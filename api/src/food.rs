//! Food search, custom foods, food logs and favorites. See docs/spec.md.
//!
//! - Nutrient values on `foods` are per serving; `food_logs` snapshot totals
//!   (`food * servings`) at write time so later food edits never rewrite history.
//! - Search merges local rows (own custom foods + cached external foods) with
//!   Open Food Facts and USDA results, which are upserted into `foods` so logs
//!   always reference local rows. Providers are injected through an
//!   `Extension<FoodProviders>`; tests supply fakes.
//! - A user's "day" is the UTC date for now.

pub mod providers;

use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::{Path, Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Extension, Json, Router,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use providers::{ExternalFood, FoodProviders};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::{collections::HashSet, sync::OnceLock};
use uuid::Uuid;
use validator::{Validate, ValidationError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/foods/search", get(search))
        .route("/foods/recent", get(recent))
        .route("/foods", post(create_food))
        .route("/food-logs", get(list_logs).post(create_log))
        .route("/food-logs/summary", get(summary))
        .route("/food-logs/{id}", put(update_log).delete(delete_log))
        .route("/favorites", get(list_favorites))
        .route(
            "/favorites/{food_id}",
            put(add_favorite).delete(remove_favorite),
        )
        .layer(middleware::from_fn(inject_default_providers))
}

/// Adds the real providers unless something (a test) already supplied some.
async fn inject_default_providers(mut req: Request, next: Next) -> Response {
    if req.extensions().get::<FoodProviders>().is_none() {
        static DEFAULT: OnceLock<FoodProviders> = OnceLock::new();
        let p = DEFAULT.get_or_init(FoodProviders::from_env).clone();
        req.extensions_mut().insert(p);
    }
    next.run(req).await
}

// ---------- errors ----------

/// `AppError` has no 404 variant; keep this module self-contained.
#[derive(Debug)]
pub enum FoodError {
    NotFound,
    App(AppError),
}

impl From<AppError> for FoodError {
    fn from(e: AppError) -> Self {
        FoodError::App(e)
    }
}
impl From<sqlx::Error> for FoodError {
    fn from(e: sqlx::Error) -> Self {
        FoodError::App(e.into())
    }
}
impl From<validator::ValidationErrors> for FoodError {
    fn from(e: validator::ValidationErrors) -> Self {
        FoodError::App(e.into())
    }
}

impl IntoResponse for FoodError {
    fn into_response(self) -> Response {
        match self {
            FoodError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "not_found", "message": "Not found" } })),
            )
                .into_response(),
            FoodError::App(e) => e.into_response(),
        }
    }
}

fn field_error(field: &str, code: &str) -> FoodError {
    FoodError::App(AppError::Validation(
        [(field.to_string(), vec![code.to_string()])].into(),
    ))
}

// ---------- types ----------

const FOOD_COLS: &str =
    "f.id, f.name, f.brand, f.kcal, f.protein_g, f.carbs_g, f.fat_g, f.serving_g, f.source, f.external_id";

#[derive(Serialize, sqlx::FromRow, Clone, Debug)]
pub struct Food {
    pub id: Uuid,
    pub name: String,
    pub brand: Option<String>,
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub serving_g: f64,
    pub source: String,
    pub external_id: Option<String>,
}

#[derive(Serialize, sqlx::FromRow, Debug)]
pub struct FoodLog {
    pub id: Uuid,
    pub food_id: Uuid,
    pub food_name: String,
    pub brand: Option<String>,
    pub logged_at: DateTime<Utc>,
    pub servings: f64,
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub meal: Option<String>,
    pub updated_at: DateTime<Utc>,
}

const LOG_SELECT: &str = "SELECT l.id, l.food_id, f.name AS food_name, f.brand, l.logged_at, \
     l.servings, l.kcal, l.protein_g, l.carbs_g, l.fat_g, l.meal, l.updated_at \
     FROM food_logs l JOIN foods f ON f.id = l.food_id";

const MEALS: [&str; 4] = ["breakfast", "lunch", "dinner", "snack"];

fn validate_meal(m: &str) -> Result<(), ValidationError> {
    if MEALS.contains(&m) {
        Ok(())
    } else {
        Err(ValidationError::new("meal"))
    }
}

/// Parses `?date=YYYY-MM-DD` (default: today, UTC) into a half-open UTC range.
fn day_range(date: Option<&str>) -> Result<(NaiveDate, DateTime<Utc>, DateTime<Utc>), FoodError> {
    let day = match date {
        Some(d) => {
            NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| field_error("date", "date"))?
        }
        None => Utc::now().date_naive(),
    };
    let start = day.and_hms_opt(0, 0, 0).expect("midnight").and_utc();
    Ok((day, start, start + Duration::days(1)))
}

/// A food the user may use: shared (external) or their own custom food.
async fn visible_food_exists(pool: &PgPool, user: Uuid, food_id: Uuid) -> Result<bool, FoodError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM foods WHERE id = $1 AND (owner_id IS NULL OR owner_id = $2)",
    )
    .bind(food_id)
    .bind(user)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

// ---------- search ----------

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

fn like_escape(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

async fn upsert_external(pool: &PgPool, f: &ExternalFood) -> Result<Food, sqlx::Error> {
    sqlx::query_as::<_, Food>(
        "INSERT INTO foods (name, brand, kcal, protein_g, carbs_g, fat_g, serving_g, source, external_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (source, external_id) WHERE external_id IS NOT NULL DO UPDATE SET \
           name = EXCLUDED.name, brand = EXCLUDED.brand, kcal = EXCLUDED.kcal, \
           protein_g = EXCLUDED.protein_g, carbs_g = EXCLUDED.carbs_g, fat_g = EXCLUDED.fat_g, \
           serving_g = EXCLUDED.serving_g \
         RETURNING id, name, brand, kcal, protein_g, carbs_g, fat_g, serving_g, source, external_id",
    )
    .bind(&f.name)
    .bind(&f.brand)
    .bind(f.kcal)
    .bind(f.protein_g)
    .bind(f.carbs_g)
    .bind(f.fat_g)
    .bind(f.serving_g)
    .bind(&f.source)
    .bind(&f.external_id)
    .fetch_one(pool)
    .await
}

async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(providers): Extension<FoodProviders>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<Food>>, FoodError> {
    let q = params.q.unwrap_or_default();
    let q = q.trim();
    if q.chars().count() < 2 {
        return Err(field_error("q", "length"));
    }
    if q.chars().count() > 100 {
        return Err(field_error("q", "length"));
    }

    // Own custom foods first, then cached external foods.
    let mut results: Vec<Food> = sqlx::query_as(&format!(
        "SELECT {FOOD_COLS} FROM foods f \
         WHERE (f.name ILIKE $1 OR f.brand ILIKE $1) AND (f.owner_id IS NULL OR f.owner_id = $2) \
         ORDER BY (f.owner_id IS NULL), length(f.name), f.name LIMIT 25"
    ))
    .bind(like_escape(q))
    .bind(user.0)
    .fetch_all(&state.pool)
    .await?;

    let mut seen: HashSet<Uuid> = results.iter().map(|f| f.id).collect();
    for ext in providers.search_all(q).await {
        match upsert_external(&state.pool, &ext).await {
            Ok(food) => {
                if seen.insert(food.id) {
                    results.push(food);
                } else if let Some(slot) = results.iter_mut().find(|r| r.id == food.id) {
                    // Already cached: swap in the refreshed row.
                    *slot = food;
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not cache external food"),
        }
    }
    Ok(Json(results))
}

async fn recent(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<Food>>, FoodError> {
    let foods = sqlx::query_as(&format!(
        "SELECT {FOOD_COLS} FROM foods f JOIN ( \
           SELECT food_id, MAX(logged_at) AS last FROM food_logs \
           WHERE user_id = $1 AND deleted_at IS NULL GROUP BY food_id \
           ORDER BY last DESC LIMIT 10) r ON r.food_id = f.id \
         ORDER BY r.last DESC"
    ))
    .bind(user.0)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(foods))
}

// ---------- custom foods ----------

#[derive(Deserialize, Validate)]
pub struct CreateFood {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(length(max = 200))]
    pub brand: Option<String>,
    #[validate(range(min = 0.0, max = 5000.0))]
    pub kcal: f64,
    #[validate(range(min = 0.0, max = 1000.0))]
    pub protein_g: f64,
    #[validate(range(min = 0.0, max = 1000.0))]
    pub carbs_g: f64,
    #[validate(range(min = 0.0, max = 1000.0))]
    pub fat_g: f64,
    #[validate(range(exclusive_min = 0.0, max = 10000.0))]
    pub serving_g: f64,
}

async fn create_food(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateFood>,
) -> Result<(StatusCode, Json<Food>), FoodError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(field_error("name", "length"));
    }
    let brand = req
        .brand
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    let food = sqlx::query_as::<_, Food>(
        "INSERT INTO foods (name, brand, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'custom', $8) \
         RETURNING id, name, brand, kcal, protein_g, carbs_g, fat_g, serving_g, source, external_id",
    )
    .bind(name)
    .bind(brand)
    .bind(req.kcal)
    .bind(req.protein_g)
    .bind(req.carbs_g)
    .bind(req.fat_g)
    .bind(req.serving_g)
    .bind(user.0)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(food)))
}

// ---------- food logs ----------

#[derive(Deserialize)]
struct DateQuery {
    date: Option<String>,
}

async fn fetch_log(pool: &PgPool, user: Uuid, id: Uuid) -> Result<Option<FoodLog>, FoodError> {
    Ok(sqlx::query_as::<_, FoodLog>(&format!(
        "{LOG_SELECT} WHERE l.id = $1 AND l.user_id = $2 AND l.deleted_at IS NULL"
    ))
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await?)
}

async fn list_logs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<DateQuery>,
) -> Result<Json<Vec<FoodLog>>, FoodError> {
    let (_, start, end) = day_range(q.date.as_deref())?;
    let logs = sqlx::query_as::<_, FoodLog>(&format!(
        "{LOG_SELECT} WHERE l.user_id = $1 AND l.deleted_at IS NULL \
         AND l.logged_at >= $2 AND l.logged_at < $3 ORDER BY l.logged_at, l.id"
    ))
    .bind(user.0)
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(logs))
}

#[derive(Deserialize, Validate)]
pub struct CreateLog {
    /// Client-generated; replaying the same id is an idempotent update.
    pub id: Option<Uuid>,
    pub food_id: Uuid,
    #[validate(range(exclusive_min = 0.0, max = 1000.0))]
    pub servings: f64,
    pub logged_at: Option<DateTime<Utc>>,
    #[validate(custom(function = "validate_meal"))]
    pub meal: Option<String>,
}

async fn create_log(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateLog>,
) -> Result<(StatusCode, Json<FoodLog>), FoodError> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    // Server clamps future timestamps to now.
    let logged_at = req.logged_at.map(|t| t.min(Utc::now()));
    // Another user's log id, a soft-deleted log, or an invisible food all yield no row.
    let row: Option<(bool,)> = sqlx::query_as(
        "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g, meal) \
         SELECT $1::uuid, $2::uuid, f.id, COALESCE($4::timestamptz, now()), $5::float8, \
           round((f.kcal * $5::float8)::numeric, 2)::float8, \
           round((f.protein_g * $5::float8)::numeric, 2)::float8, \
           round((f.carbs_g * $5::float8)::numeric, 2)::float8, \
           round((f.fat_g * $5::float8)::numeric, 2)::float8, \
           $6::text \
         FROM foods f WHERE f.id = $3 AND (f.owner_id IS NULL OR f.owner_id = $2) \
         ON CONFLICT (id) DO UPDATE SET \
           food_id = EXCLUDED.food_id, \
           logged_at = COALESCE($4::timestamptz, food_logs.logged_at), \
           servings = EXCLUDED.servings, kcal = EXCLUDED.kcal, protein_g = EXCLUDED.protein_g, \
           carbs_g = EXCLUDED.carbs_g, fat_g = EXCLUDED.fat_g, meal = EXCLUDED.meal, \
           updated_at = now() \
         WHERE food_logs.user_id = EXCLUDED.user_id AND food_logs.deleted_at IS NULL \
         RETURNING (xmax = 0)",
    )
    .bind(id)
    .bind(user.0)
    .bind(req.food_id)
    .bind(logged_at)
    .bind(req.servings)
    .bind(&req.meal)
    .fetch_optional(&state.pool)
    .await?;
    let (inserted,) = row.ok_or(FoodError::NotFound)?;
    let log = fetch_log(&state.pool, user.0, id)
        .await?
        .ok_or(FoodError::NotFound)?;
    let status = if inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(log)))
}

#[derive(Deserialize, Validate)]
pub struct UpdateLog {
    #[validate(range(exclusive_min = 0.0, max = 1000.0))]
    pub servings: Option<f64>,
    pub logged_at: Option<DateTime<Utc>>,
    #[validate(custom(function = "validate_meal"))]
    pub meal: Option<String>,
}

async fn update_log(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateLog>,
) -> Result<Json<FoodLog>, FoodError> {
    let logged_at = req.logged_at.map(|t| t.min(Utc::now()));
    let row: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE food_logs l SET \
           servings = COALESCE($3::float8, l.servings), \
           logged_at = COALESCE($4::timestamptz, l.logged_at), \
           meal = COALESCE($5::text, l.meal), \
           kcal = round((f.kcal * COALESCE($3::float8, l.servings))::numeric, 2)::float8, \
           protein_g = round((f.protein_g * COALESCE($3::float8, l.servings))::numeric, 2)::float8, \
           carbs_g = round((f.carbs_g * COALESCE($3::float8, l.servings))::numeric, 2)::float8, \
           fat_g = round((f.fat_g * COALESCE($3::float8, l.servings))::numeric, 2)::float8, \
           updated_at = now() \
         FROM foods f \
         WHERE f.id = l.food_id AND l.id = $1 AND l.user_id = $2 AND l.deleted_at IS NULL \
         RETURNING l.id",
    )
    .bind(id)
    .bind(user.0)
    .bind(req.servings)
    .bind(logged_at)
    .bind(&req.meal)
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(FoodError::NotFound)?;
    let log = fetch_log(&state.pool, user.0, id)
        .await?
        .ok_or(FoodError::NotFound)?;
    Ok(Json(log))
}

async fn delete_log(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, FoodError> {
    let res = sqlx::query(
        "UPDATE food_logs SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.0)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(FoodError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct Summary {
    date: NaiveDate,
    kcal: f64,
    protein_g: f64,
    carbs_g: f64,
    fat_g: f64,
    entries: i64,
}

async fn summary(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<DateQuery>,
) -> Result<Json<Summary>, FoodError> {
    let (date, start, end) = day_range(q.date.as_deref())?;
    let (kcal, protein_g, carbs_g, fat_g, entries): (f64, f64, f64, f64, i64) = sqlx::query_as(
        "SELECT round(COALESCE(SUM(kcal), 0)::numeric, 2)::float8, \
                round(COALESCE(SUM(protein_g), 0)::numeric, 2)::float8, \
                round(COALESCE(SUM(carbs_g), 0)::numeric, 2)::float8, \
                round(COALESCE(SUM(fat_g), 0)::numeric, 2)::float8, \
                COUNT(*) \
         FROM food_logs WHERE user_id = $1 AND deleted_at IS NULL \
         AND logged_at >= $2 AND logged_at < $3",
    )
    .bind(user.0)
    .bind(start)
    .bind(end)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(Summary {
        date,
        kcal,
        protein_g,
        carbs_g,
        fat_g,
        entries,
    }))
}

// ---------- favorites ----------

async fn list_favorites(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<Food>>, FoodError> {
    let foods = sqlx::query_as(&format!(
        "SELECT {FOOD_COLS} FROM favorite_foods ff JOIN foods f ON f.id = ff.food_id \
         WHERE ff.user_id = $1 AND (f.owner_id IS NULL OR f.owner_id = $1) \
         ORDER BY ff.created_at DESC, f.id"
    ))
    .bind(user.0)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(foods))
}

async fn add_favorite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(food_id): Path<Uuid>,
) -> Result<StatusCode, FoodError> {
    if !visible_food_exists(&state.pool, user.0, food_id).await? {
        return Err(FoodError::NotFound);
    }
    sqlx::query(
        "INSERT INTO favorite_foods (user_id, food_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user.0)
    .bind(food_id)
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_favorite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(food_id): Path<Uuid>,
) -> Result<StatusCode, FoodError> {
    sqlx::query("DELETE FROM favorite_foods WHERE user_id = $1 AND food_id = $2")
        .bind(user.0)
        .bind(food_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

//! Body composition (InBody scans or manual entries), soft-deleted, client ids.
//! Every save mirrors the weight into `weight_logs` (same id) in one transaction.
use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use validator::{Validate, ValidationError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/body-composition", get(list).post(create))
        .route("/body-composition/{id}", put(update).delete(remove))
}

enum ApiError {
    App(AppError),
    NotFound,
}

impl<E: Into<AppError>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError::App(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::App(e) => e.into_response(),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "not_found", "message": "Not found" } })),
            )
                .into_response(),
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BodyComposition {
    pub id: Uuid,
    pub measured_at: DateTime<Utc>,
    pub weight_kg: f64,
    pub body_fat_pct: Option<f64>,
    pub skeletal_muscle_kg: Option<f64>,
    pub fat_mass_kg: Option<f64>,
    pub visceral_fat_level: Option<i16>,
    pub bmr_kcal: Option<i32>,
    pub body_water_pct: Option<f64>,
    pub source: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UpsertRow {
    #[sqlx(flatten)]
    row: BodyComposition,
    inserted: bool,
}

const COLUMNS: &str = "id, measured_at, weight_kg, body_fat_pct, skeletal_muscle_kg, fat_mass_kg, \
                       visceral_fat_level, bmr_kcal, body_water_pct, source, updated_at";

fn validate_source(s: &str) -> Result<(), ValidationError> {
    match s {
        "inbody" | "manual" => Ok(()),
        _ => Err(ValidationError::new("source")),
    }
}

#[derive(Deserialize, Validate)]
struct BodyInput {
    /// Client id; ignored by PUT.
    id: Option<Uuid>,
    measured_at: Option<DateTime<Utc>>,
    #[validate(range(min = 20.0, max = 400.0))]
    weight_kg: f64,
    #[validate(range(min = 2.0, max = 75.0))]
    body_fat_pct: Option<f64>,
    #[validate(range(min = 5.0, max = 100.0))]
    skeletal_muscle_kg: Option<f64>,
    #[validate(range(min = 0.0, max = 400.0))]
    fat_mass_kg: Option<f64>,
    #[validate(range(min = 1, max = 30))]
    visceral_fat_level: Option<i16>,
    #[validate(range(min = 600, max = 5000))]
    bmr_kcal: Option<i32>,
    #[validate(range(min = 20.0, max = 80.0))]
    body_water_pct: Option<f64>,
    #[validate(custom(function = "validate_source"))]
    source: Option<String>,
}

#[derive(Deserialize)]
struct RangeQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<BodyComposition>>, ApiError> {
    let rows = sqlx::query_as::<_, BodyComposition>(&format!(
        "SELECT {COLUMNS} FROM body_compositions \
         WHERE user_id = $1 AND deleted_at IS NULL \
           AND ($2::timestamptz IS NULL OR measured_at >= $2) \
           AND ($3::timestamptz IS NULL OR measured_at <= $3) \
         ORDER BY measured_at DESC, created_at DESC LIMIT 1000"
    ))
    .bind(user.0)
    .bind(q.from)
    .bind(q.to)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Upserts the matching weight log (same id). A weight row with this id owned by
/// someone else is a 404; the surrounding transaction then rolls back.
async fn mirror_weight(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    b: &BodyComposition,
) -> Result<(), ApiError> {
    let res = sqlx::query(
        "INSERT INTO weight_logs (id, user_id, measured_at, kg) \
         VALUES ($1, $2, $3, $4::float8::numeric(5,2)) \
         ON CONFLICT (id) DO UPDATE SET measured_at = EXCLUDED.measured_at, \
           kg = EXCLUDED.kg, deleted_at = NULL, updated_at = now() \
         WHERE weight_logs.user_id = EXCLUDED.user_id",
    )
    .bind(b.id)
    .bind(user_id)
    .bind(b.measured_at)
    .bind(b.weight_kg)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<BodyInput>,
) -> Result<(StatusCode, Json<BodyComposition>), ApiError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query_as::<_, UpsertRow>(&format!(
        "INSERT INTO body_compositions (id, user_id, measured_at, weight_kg, body_fat_pct, \
           skeletal_muscle_kg, fat_mass_kg, visceral_fat_level, bmr_kcal, body_water_pct, source) \
         VALUES (COALESCE($1, gen_random_uuid()), $2, COALESCE($3::timestamptz, now()), \
                 $4, $5, $6, $7, $8, $9, $10, COALESCE($11, 'manual')) \
         ON CONFLICT (id) DO UPDATE SET \
           measured_at = COALESCE($3::timestamptz, body_compositions.measured_at), \
           weight_kg = EXCLUDED.weight_kg, body_fat_pct = EXCLUDED.body_fat_pct, \
           skeletal_muscle_kg = EXCLUDED.skeletal_muscle_kg, fat_mass_kg = EXCLUDED.fat_mass_kg, \
           visceral_fat_level = EXCLUDED.visceral_fat_level, bmr_kcal = EXCLUDED.bmr_kcal, \
           body_water_pct = EXCLUDED.body_water_pct, source = EXCLUDED.source, \
           updated_at = now() \
         WHERE body_compositions.user_id = EXCLUDED.user_id \
           AND body_compositions.deleted_at IS NULL \
         RETURNING {COLUMNS}, (xmax = 0) AS inserted"
    ))
    .bind(req.id)
    .bind(user.0)
    .bind(req.measured_at)
    .bind(req.weight_kg)
    .bind(req.body_fat_pct)
    .bind(req.skeletal_muscle_kg)
    .bind(req.fat_mass_kg)
    .bind(req.visceral_fat_level)
    .bind(req.bmr_kcal)
    .bind(req.body_water_pct)
    .bind(req.source)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    mirror_weight(&mut tx, user.0, &row.row).await?;
    tx.commit().await?;
    let status = if row.inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(row.row)))
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<BodyInput>,
) -> Result<Json<BodyComposition>, ApiError> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query_as::<_, BodyComposition>(&format!(
        "UPDATE body_compositions SET \
           measured_at = COALESCE($3::timestamptz, measured_at), weight_kg = $4, \
           body_fat_pct = $5, skeletal_muscle_kg = $6, fat_mass_kg = $7, \
           visceral_fat_level = $8, bmr_kcal = $9, body_water_pct = $10, \
           source = COALESCE($11, source), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL \
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user.0)
    .bind(req.measured_at)
    .bind(req.weight_kg)
    .bind(req.body_fat_pct)
    .bind(req.skeletal_muscle_kg)
    .bind(req.fat_mass_kg)
    .bind(req.visceral_fat_level)
    .bind(req.bmr_kcal)
    .bind(req.body_water_pct)
    .bind(req.source)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    mirror_weight(&mut tx, user.0, &row).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE body_compositions SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.0)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query(
        "UPDATE weight_logs SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Latest non-deleted scan at or before `now` (any age).
pub async fn latest(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<BodyComposition>, sqlx::Error> {
    sqlx::query_as::<_, BodyComposition>(&format!(
        "SELECT {COLUMNS} FROM body_compositions \
         WHERE user_id = $1 AND deleted_at IS NULL AND measured_at <= $2 \
         ORDER BY measured_at DESC, created_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .bind(now)
    .fetch_optional(pool)
    .await
}

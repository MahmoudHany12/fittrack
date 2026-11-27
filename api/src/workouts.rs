//! Workouts and the exercise catalog. See docs/spec.md. Routes use full paths.
//!
//! Every query is scoped to the authenticated user. Another user's workout is
//! reported as 404 so ids cannot be probed. Creating with a client supplied
//! `id` is idempotent for its owner and never touches another user's row.

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
use sqlx::{FromRow, PgConnection};
use std::collections::HashMap;
use uuid::Uuid;
use validator::{Validate, ValidationError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/exercises", get(list_exercises).post(create_exercise))
        .route("/workouts", get(list_workouts).post(create_workout))
        .route("/workouts/{id}", put(update_workout).delete(delete_workout))
}

// ---------- errors ----------

/// `AppError` has no 404 variant, so this module wraps it.
pub enum WorkoutError {
    NotFound,
    App(AppError),
}

impl From<AppError> for WorkoutError {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

impl From<sqlx::Error> for WorkoutError {
    fn from(e: sqlx::Error) -> Self {
        Self::App(e.into())
    }
}

impl IntoResponse for WorkoutError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "not_found", "message": "Not found" } })),
            )
                .into_response(),
            Self::App(e) => e.into_response(),
        }
    }
}

fn invalid(field: &str, code: &'static str) -> WorkoutError {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert(field.to_string(), vec![code.to_string()]);
    WorkoutError::App(AppError::Validation(fields))
}

// ---------- exercises ----------

const MUSCLE_GROUPS: [&str; 7] = [
    "chest",
    "back",
    "legs",
    "shoulders",
    "arms",
    "core",
    "cardio",
];

#[derive(Serialize, FromRow)]
pub struct Exercise {
    pub id: Uuid,
    pub name: String,
    pub muscle_group: String,
    /// True for the signed-in user's own exercises, false for the catalog.
    pub custom: bool,
}

#[derive(Deserialize)]
struct ExerciseQuery {
    q: Option<String>,
}

/// Escape LIKE wildcards so the search text is matched literally.
fn like_pattern(q: &str) -> String {
    let mut out = String::from("%");
    for c in q.trim().chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

async fn list_exercises(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<ExerciseQuery>,
) -> Result<Json<Vec<Exercise>>, WorkoutError> {
    let pattern = like_pattern(query.q.as_deref().unwrap_or(""));
    let rows = sqlx::query_as::<_, Exercise>(
        "SELECT id, name, muscle_group, (owner_id IS NOT NULL) AS custom
         FROM exercises
         WHERE (owner_id IS NULL OR owner_id = $1) AND name ILIKE $2 ESCAPE '\\'
         ORDER BY name
         LIMIT 200",
    )
    .bind(user_id)
    .bind(pattern)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

fn not_blank(v: &str) -> Result<(), ValidationError> {
    if v.trim().is_empty() {
        return Err(ValidationError::new("blank"));
    }
    Ok(())
}

fn muscle_group(v: &str) -> Result<(), ValidationError> {
    if MUSCLE_GROUPS.contains(&v) {
        Ok(())
    } else {
        Err(ValidationError::new("muscle_group"))
    }
}

#[derive(Deserialize, Validate)]
pub struct NewExercise {
    #[validate(length(min = 1, max = 80), custom(function = "not_blank"))]
    pub name: String,
    #[validate(custom(function = "muscle_group"))]
    pub muscle_group: String,
}

async fn create_exercise(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ValidatedJson(body): ValidatedJson<NewExercise>,
) -> Result<(StatusCode, Json<Exercise>), WorkoutError> {
    let name = body.name.trim();
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM exercises
                        WHERE lower(name) = lower($1) AND (owner_id IS NULL OR owner_id = $2))",
    )
    .bind(name)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;
    if taken {
        return Err(AppError::Conflict("An exercise with that name already exists").into());
    }
    let row = sqlx::query_as::<_, Exercise>(
        "INSERT INTO exercises (name, muscle_group, owner_id) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING
         RETURNING id, name, muscle_group, true AS custom",
    )
    .bind(name)
    .bind(&body.muscle_group)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Conflict(
        "An exercise with that name already exists",
    ))?;
    Ok((StatusCode::CREATED, Json(row)))
}

// ---------- workouts ----------

#[derive(Serialize, Deserialize, Validate)]
pub struct SetInput {
    pub exercise_id: Uuid,
    #[validate(range(min = 1, max = 1000))]
    pub reps: i32,
    #[validate(range(min = 0.0, max = 1000.0))]
    pub weight_kg: f64,
}

fn validate_sets(sets: &[SetInput]) -> Result<(), ValidationError> {
    if sets.iter().all(|s| s.validate().is_ok()) {
        Ok(())
    } else {
        Err(ValidationError::new("invalid_set"))
    }
}

#[derive(Deserialize, Validate)]
pub struct WorkoutInput {
    /// Client generated id; makes creation idempotent. Ignored by PUT.
    pub id: Option<Uuid>,
    /// Defaults to now. Future values are clamped to now.
    pub started_at: Option<DateTime<Utc>>,
    #[validate(range(min = 1, max = 86400))]
    pub duration_s: i32,
    #[validate(length(max = 500))]
    pub note: Option<String>,
    #[serde(default)]
    #[validate(length(max = 100), custom(function = "validate_sets"))]
    pub sets: Vec<SetInput>,
}

#[derive(Serialize, FromRow)]
pub struct WorkoutSet {
    pub id: Uuid,
    pub exercise_id: Uuid,
    pub reps: i32,
    pub weight_kg: f64,
    pub position: i32,
}

#[derive(Serialize, FromRow)]
struct WorkoutRow {
    id: Uuid,
    started_at: DateTime<Utc>,
    duration_s: i32,
    note: Option<String>,
    source: String,
    external_id: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Workout {
    #[serde(flatten)]
    row: WorkoutRow,
    sets: Vec<WorkoutSet>,
}

const WORKOUT_COLS: &str = "id, started_at, duration_s, note, source, external_id, updated_at";

#[derive(FromRow)]
struct SetRow {
    workout_id: Uuid,
    #[sqlx(flatten)]
    set: WorkoutSet,
}

/// Load sets for the given workouts, joined to the owner so the user filter
/// applies here as well.
async fn load_sets(
    conn: &mut PgConnection,
    user_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<WorkoutSet>>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SetRow>(
        "SELECT s.workout_id, s.id, s.exercise_id, s.reps, s.weight_kg, s.position
         FROM workout_sets s JOIN workouts w ON w.id = s.workout_id
         WHERE w.user_id = $1 AND w.deleted_at IS NULL AND s.workout_id = ANY($2)
         ORDER BY s.workout_id, s.position",
    )
    .bind(user_id)
    .bind(ids)
    .fetch_all(conn)
    .await?;
    let mut map: HashMap<Uuid, Vec<WorkoutSet>> = HashMap::new();
    for r in rows {
        map.entry(r.workout_id).or_default().push(r.set);
    }
    Ok(map)
}

async fn fetch_workout(
    conn: &mut PgConnection,
    user_id: Uuid,
    id: Uuid,
) -> Result<Workout, WorkoutError> {
    let row = sqlx::query_as::<_, WorkoutRow>(&format!(
        "SELECT {WORKOUT_COLS} FROM workouts WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(WorkoutError::NotFound)?;
    let sets = load_sets(conn, user_id, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();
    Ok(Workout { row, sets })
}

#[derive(Deserialize)]
struct RangeQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

async fn list_workouts(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(range): Query<RangeQuery>,
) -> Result<Json<Vec<Workout>>, WorkoutError> {
    let mut conn = state.pool.acquire().await?;
    let rows = sqlx::query_as::<_, WorkoutRow>(&format!(
        "SELECT {WORKOUT_COLS} FROM workouts
         WHERE user_id = $1 AND deleted_at IS NULL
           AND ($2::timestamptz IS NULL OR started_at >= $2)
           AND ($3::timestamptz IS NULL OR started_at <= $3)
         ORDER BY started_at DESC, id
         LIMIT 500"
    ))
    .bind(user_id)
    .bind(range.from)
    .bind(range.to)
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut sets = load_sets(&mut conn, user_id, &ids).await?;
    let out = rows
        .into_iter()
        .map(|row| {
            let s = sets.remove(&row.id).unwrap_or_default();
            Workout { row, sets: s }
        })
        .collect();
    Ok(Json(out))
}

/// Every exercise must be in the catalog or owned by this user.
async fn check_exercises(
    conn: &mut PgConnection,
    user_id: Uuid,
    sets: &[SetInput],
) -> Result<(), WorkoutError> {
    let mut ids: Vec<Uuid> = sets.iter().map(|s| s.exercise_id).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM exercises WHERE id = ANY($1) AND (owner_id IS NULL OR owner_id = $2)",
    )
    .bind(&ids)
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    if found as usize != ids.len() {
        return Err(invalid("sets", "unknown_exercise"));
    }
    Ok(())
}

async fn replace_sets(
    conn: &mut PgConnection,
    workout_id: Uuid,
    sets: &[SetInput],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM workout_sets WHERE workout_id = $1")
        .bind(workout_id)
        .execute(&mut *conn)
        .await?;
    if sets.is_empty() {
        return Ok(());
    }
    let exercise_ids: Vec<Uuid> = sets.iter().map(|s| s.exercise_id).collect();
    let reps: Vec<i32> = sets.iter().map(|s| s.reps).collect();
    let weights: Vec<f64> = sets.iter().map(|s| s.weight_kg).collect();
    let positions: Vec<i32> = (0..sets.len() as i32).collect();
    sqlx::query(
        "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position)
         SELECT $1, * FROM UNNEST($2::uuid[], $3::int[], $4::float8[], $5::int[])",
    )
    .bind(workout_id)
    .bind(exercise_ids)
    .bind(reps)
    .bind(weights)
    .bind(positions)
    .execute(conn)
    .await?;
    Ok(())
}

async fn create_workout(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ValidatedJson(body): ValidatedJson<WorkoutInput>,
) -> Result<(StatusCode, Json<Workout>), WorkoutError> {
    let id = body.id.unwrap_or_else(Uuid::new_v4);
    let mut tx = state.pool.begin().await?;
    check_exercises(&mut tx, user_id, &body.sets).await?;
    // Upsert, but only over the caller's own live row. A conflicting id owned
    // by someone else (or already deleted) matches nothing and becomes 404.
    let existed: Option<bool> = sqlx::query_scalar(
        "INSERT INTO workouts (id, user_id, started_at, duration_s, note, updated_at)
         VALUES ($1, $2, LEAST(COALESCE($3, now()), now()), $4, $5, now())
         ON CONFLICT (id) DO UPDATE
           SET started_at = LEAST(COALESCE($3, workouts.started_at), now()),
               duration_s = EXCLUDED.duration_s,
               note = EXCLUDED.note,
               updated_at = now()
           WHERE workouts.user_id = $2 AND workouts.deleted_at IS NULL
         RETURNING (xmax::text::bigint <> 0)",
    )
    .bind(id)
    .bind(user_id)
    .bind(body.started_at)
    .bind(body.duration_s)
    .bind(body.note.as_deref())
    .fetch_optional(&mut *tx)
    .await?;
    let existed = existed.ok_or(WorkoutError::NotFound)?;
    replace_sets(&mut tx, id, &body.sets).await?;
    let workout = fetch_workout(&mut tx, user_id, id).await?;
    tx.commit().await?;
    let status = if existed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(workout)))
}

async fn update_workout(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<WorkoutInput>,
) -> Result<Json<Workout>, WorkoutError> {
    let mut tx = state.pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE workouts
         SET started_at = LEAST(COALESCE($3, started_at), now()),
             duration_s = $4, note = $5, updated_at = now()
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(body.started_at)
    .bind(body.duration_s)
    .bind(body.note.as_deref())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(WorkoutError::NotFound);
    }
    check_exercises(&mut tx, user_id, &body.sets).await?;
    replace_sets(&mut tx, id, &body.sets).await?;
    let workout = fetch_workout(&mut tx, user_id, id).await?;
    tx.commit().await?;
    Ok(Json(workout))
}

async fn delete_workout(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, WorkoutError> {
    let res = sqlx::query(
        "UPDATE workouts SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(WorkoutError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

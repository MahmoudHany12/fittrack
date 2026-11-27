//! Weight logs (soft-deleted, client-generated ids) and the 7-day weight trend.
use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/weight", get(list).post(create))
        .route("/weight/{id}", put(update).delete(remove))
}

/// `AppError` has no 404; cross-user and missing rows are indistinguishable.
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
pub struct WeightLog {
    pub id: Uuid,
    pub measured_at: DateTime<Utc>,
    pub kg: f64,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UpsertRow {
    #[sqlx(flatten)]
    log: WeightLog,
    inserted: bool,
}

const COLUMNS: &str = "id, measured_at, kg::float8 AS kg, updated_at, created_at";

#[derive(Deserialize)]
struct RangeQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Validate)]
struct CreateWeight {
    id: Option<Uuid>,
    measured_at: Option<DateTime<Utc>>,
    #[validate(range(min = 20.0, max = 400.0))]
    kg: f64,
}

#[derive(Deserialize, Validate)]
struct UpdateWeight {
    measured_at: Option<DateTime<Utc>>,
    #[validate(range(min = 20.0, max = 400.0))]
    kg: f64,
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<WeightLog>>, ApiError> {
    let rows = sqlx::query_as::<_, WeightLog>(&format!(
        "SELECT {COLUMNS} FROM weight_logs \
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

/// Idempotent by `id`: replaying the same request updates the owner's row.
/// An id that belongs to another user (or is soft-deleted) is a 404 and is
/// never modified.
async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateWeight>,
) -> Result<(StatusCode, Json<WeightLog>), ApiError> {
    let row = sqlx::query_as::<_, UpsertRow>(&format!(
        "INSERT INTO weight_logs (id, user_id, measured_at, kg) \
         VALUES (COALESCE($1, gen_random_uuid()), $2, COALESCE($3::timestamptz, now()), \
                 $4::float8::numeric(5,2)) \
         ON CONFLICT (id) DO UPDATE SET \
           measured_at = COALESCE($3::timestamptz, weight_logs.measured_at), \
           kg = EXCLUDED.kg, \
           updated_at = now() \
         WHERE weight_logs.user_id = EXCLUDED.user_id AND weight_logs.deleted_at IS NULL \
         RETURNING {COLUMNS}, (xmax = 0) AS inserted"
    ))
    .bind(req.id)
    .bind(user.0)
    .bind(req.measured_at)
    .bind(req.kg)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let status = if row.inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(row.log)))
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateWeight>,
) -> Result<Json<WeightLog>, ApiError> {
    sqlx::query_as::<_, WeightLog>(&format!(
        "UPDATE weight_logs SET kg = $3::float8::numeric(5,2), \
           measured_at = COALESCE($4::timestamptz, measured_at), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL \
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user.0)
    .bind(req.kg)
    .bind(req.measured_at)
    .fetch_optional(&state.pool)
    .await?
    .map(Json)
    .ok_or(ApiError::NotFound)
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let res = sqlx::query(
        "UPDATE weight_logs SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.0)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- trend ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Up,
    Flat,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeightTrend {
    pub current_kg: Option<f64>,
    pub trend: Trend,
}

/// Dead zone in grams: changes of at most 200 g count as flat.
const DEAD_ZONE_G: i64 = 200;

/// Compare the mean of the last 7 days with the mean of the 7 days before.
/// Flat when either window has no data.
pub fn compute_trend(now: DateTime<Utc>, samples: &[(DateTime<Utc>, f64)]) -> Trend {
    let week_ago = now - Duration::days(7);
    let two_weeks_ago = now - Duration::days(14);
    let mean = |lo: DateTime<Utc>, hi: DateTime<Utc>| {
        let v: Vec<f64> = samples
            .iter()
            .filter(|(t, _)| *t > lo && *t <= hi)
            .map(|(_, kg)| *kg)
            .collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
    };
    let (Some(recent), Some(previous)) = (mean(week_ago, now), mean(two_weeks_ago, week_ago))
    else {
        return Trend::Flat;
    };
    let diff_g = ((recent - previous) * 1000.0).round() as i64;
    match diff_g {
        d if d > DEAD_ZONE_G => Trend::Up,
        d if d < -DEAD_ZONE_G => Trend::Down,
        _ => Trend::Flat,
    }
}

pub async fn weight_trend(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<WeightTrend, sqlx::Error> {
    let current: Option<(f64,)> = sqlx::query_as(
        "SELECT kg::float8 FROM weight_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND measured_at <= $2 \
         ORDER BY measured_at DESC, created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let samples: Vec<(DateTime<Utc>, f64)> = sqlx::query_as(
        "SELECT measured_at, kg::float8 FROM weight_logs \
         WHERE user_id = $1 AND deleted_at IS NULL \
           AND measured_at > $2 AND measured_at <= $3",
    )
    .bind(user_id)
    .bind(now - Duration::days(14))
    .bind(now)
    .fetch_all(pool)
    .await?;
    Ok(WeightTrend {
        current_kg: current.map(|c| c.0),
        trend: compute_trend(now, &samples),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(now: DateTime<Utc>, days: i64, kg: f64) -> (DateTime<Utc>, f64) {
        (now - Duration::days(days), kg)
    }

    #[test]
    fn dead_zone_boundary_is_flat() {
        let now = Utc::now();
        let s = [at(now, 10, 80.0), at(now, 1, 80.2)];
        assert_eq!(compute_trend(now, &s), Trend::Flat);
        let s = [at(now, 10, 80.0), at(now, 1, 80.21)];
        assert_eq!(compute_trend(now, &s), Trend::Up);
        let s = [at(now, 10, 80.0), at(now, 1, 79.79)];
        assert_eq!(compute_trend(now, &s), Trend::Down);
    }

    #[test]
    fn needs_both_windows() {
        let now = Utc::now();
        assert_eq!(compute_trend(now, &[]), Trend::Flat);
        assert_eq!(compute_trend(now, &[at(now, 1, 90.0)]), Trend::Flat);
        assert_eq!(compute_trend(now, &[at(now, 10, 90.0)]), Trend::Flat);
    }
}

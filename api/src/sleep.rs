//! Sleep logs. See docs/spec.md. Routes use full paths (e.g. "/sleep").
//!
//! - `POST /sleep` is an idempotent upsert by the (optional) client-supplied
//!   `id`. An id that belongs to another user is answered with 404, so ids of
//!   other users are never revealed or overwritten.
//! - Every query filters by the authenticated user; cross-user access is 404.
//! - Deletes are soft (`deleted_at` + `updated_at`) so they can sync.

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
use std::collections::BTreeMap;
use uuid::Uuid;
use validator::{Validate, ValidationError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sleep", get(list).post(create))
        .route("/sleep/{id}", put(update).delete(remove))
}

const MAX_SLEEP: i64 = 24 * 60 * 60;
const MAX_ROWS: i64 = 500;

/// `AppError` has no 404 variant; keep it local to avoid touching shared files.
pub enum SleepError {
    App(AppError),
    NotFound,
}

impl From<AppError> for SleepError {
    fn from(e: AppError) -> Self {
        SleepError::App(e)
    }
}

impl From<sqlx::Error> for SleepError {
    fn from(e: sqlx::Error) -> Self {
        SleepError::App(e.into())
    }
}

impl IntoResponse for SleepError {
    fn into_response(self) -> Response {
        match self {
            SleepError::App(e) => e.into_response(),
            SleepError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "not_found", "message": "Not found" } })),
            )
                .into_response(),
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SleepLog {
    pub id: Uuid,
    pub bedtime: DateTime<Utc>,
    pub wake_time: DateTime<Utc>,
    pub quality: Option<i16>,
    pub source: String,
    pub external_id: Option<String>,
    pub duration_minutes: i32,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

const COLS: &str = "id, bedtime, wake_time, quality, source, external_id, \
    FLOOR(EXTRACT(EPOCH FROM (wake_time - bedtime)) / 60)::int4 AS duration_minutes, \
    updated_at, created_at";

#[derive(Deserialize, Validate)]
pub struct CreateSleep {
    pub id: Option<Uuid>,
    pub bedtime: DateTime<Utc>,
    pub wake_time: DateTime<Utc>,
    #[validate(range(min = 1, max = 5))]
    pub quality: Option<i16>,
    #[validate(custom(function = "validate_source"))]
    pub source: Option<String>,
    #[validate(length(min = 1, max = 200))]
    pub external_id: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct UpdateSleep {
    pub bedtime: DateTime<Utc>,
    pub wake_time: DateTime<Utc>,
    #[validate(range(min = 1, max = 5))]
    pub quality: Option<i16>,
}

fn validate_source(s: &str) -> Result<(), ValidationError> {
    match s {
        "manual" | "health" => Ok(()),
        _ => Err(ValidationError::new("invalid_source")),
    }
}

/// Cross-field checks: wake after bedtime, at most 24 h, not far in the future.
fn check_times(
    bedtime: DateTime<Utc>,
    wake_time: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if wake_time <= bedtime {
        fields.insert("wake_time".into(), vec!["before_bedtime".into()]);
    } else if (wake_time - bedtime).num_seconds() > MAX_SLEEP {
        fields.insert("wake_time".into(), vec!["too_long".into()]);
    }
    let limit = now + Duration::days(1);
    if wake_time > limit {
        fields
            .entry("wake_time".into())
            .or_default()
            .push("in_future".into());
    }
    if bedtime > limit {
        fields.insert("bedtime".into(), vec!["in_future".into()]);
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(fields))
    }
}

#[derive(Deserialize)]
struct Range {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(range): Query<Range>,
) -> Result<Json<Vec<SleepLog>>, SleepError> {
    let rows = sqlx::query_as::<_, SleepLog>(&format!(
        "SELECT {COLS} FROM sleep_logs \
         WHERE user_id = $1 AND deleted_at IS NULL \
           AND ($2::timestamptz IS NULL OR wake_time >= $2) \
           AND ($3::timestamptz IS NULL OR wake_time <= $3) \
         ORDER BY wake_time DESC, id LIMIT $4"
    ))
    .bind(user.0)
    .bind(range.from)
    .bind(range.to)
    .bind(MAX_ROWS)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(sqlx::FromRow)]
struct Upserted {
    #[sqlx(flatten)]
    log: SleepLog,
    inserted: bool,
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateSleep>,
) -> Result<Response, SleepError> {
    check_times(req.bedtime, req.wake_time, Utc::now())?;
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    // The WHERE clause makes the update a no-op for another user's row, in which
    // case nothing is returned and we answer 404.
    let res = sqlx::query_as::<_, Upserted>(&format!(
        "INSERT INTO sleep_logs (id, user_id, bedtime, wake_time, quality, source, external_id) \
         VALUES ($1, $2, $3, $4, $5, COALESCE($6, 'manual'), $7) \
         ON CONFLICT (id) DO UPDATE SET \
            bedtime = EXCLUDED.bedtime, wake_time = EXCLUDED.wake_time, \
            quality = EXCLUDED.quality, deleted_at = NULL, updated_at = now() \
         WHERE sleep_logs.user_id = EXCLUDED.user_id \
         RETURNING {COLS}, (xmax = 0) AS inserted"
    ))
    .bind(id)
    .bind(user.0)
    .bind(req.bedtime)
    .bind(req.wake_time)
    .bind(req.quality)
    .bind(req.source)
    .bind(req.external_id)
    .fetch_optional(&state.pool)
    .await;
    let row = match res {
        Ok(Some(r)) => r,
        Ok(None) => return Err(SleepError::NotFound),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::Conflict("This sleep entry was already imported").into());
        }
        Err(e) => return Err(e.into()),
    };
    let status = if row.inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(row.log)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateSleep>,
) -> Result<Json<SleepLog>, SleepError> {
    check_times(req.bedtime, req.wake_time, Utc::now())?;
    sqlx::query_as::<_, SleepLog>(&format!(
        "UPDATE sleep_logs SET bedtime = $3, wake_time = $4, quality = $5, updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL \
         RETURNING {COLS}"
    ))
    .bind(id)
    .bind(user.0)
    .bind(req.bedtime)
    .bind(req.wake_time)
    .bind(req.quality)
    .fetch_optional(&state.pool)
    .await?
    .map(Json)
    .ok_or(SleepError::NotFound)
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, SleepError> {
    let res = sqlx::query(
        "UPDATE sleep_logs SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.0)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(SleepError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Summary of the most recent sleep, for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SleepSummary {
    pub minutes: i32,
    pub quality: Option<i16>,
    pub wake_time: DateTime<Utc>,
}

/// Most recent non-deleted sleep whose `wake_time` is within the last 36 hours
/// (and not after `now`).
pub async fn last_night(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<SleepSummary>, sqlx::Error> {
    sqlx::query_as::<_, SleepSummary>(
        "SELECT FLOOR(EXTRACT(EPOCH FROM (wake_time - bedtime)) / 60)::int4 AS minutes, \
                quality, wake_time \
         FROM sleep_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND wake_time <= $2 AND wake_time > $3 \
         ORDER BY wake_time DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(now)
    .bind(now - Duration::hours(36))
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_rules() {
        let now = Utc::now();
        let h = Duration::hours;
        assert!(check_times(now - h(8), now, now).is_ok());
        assert!(check_times(now - h(24), now, now).is_ok());
        assert!(check_times(now - h(25), now, now).is_err());
        assert!(check_times(now, now - h(1), now).is_err());
        assert!(check_times(now, now, now).is_err());
        assert!(check_times(now + h(30), now + h(38), now).is_err());
        assert!(check_times(now - h(8), now + h(15), now).is_ok());
    }
}

//! Health import. See docs/spec.md. Routes use full paths.
//!
//! - Consent is per category and defaults to off. Revoking consent
//!   (`granted = false`) stops future imports; already imported data is kept
//!   (users delete it through the normal workout / sleep screens).
//! - `POST /health/import` is idempotent: rows are deduplicated by
//!   (user, source/category, external_id), always scoped to the caller, so one
//!   user's external ids can never collide with or touch another user's rows.
//! - Health data is sensitive: payloads are never logged.

use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health/consents", get(get_consents).put(put_consents))
        .route("/health/import", post(import))
}

pub const MAX_ITEMS: usize = 1000;
const MAX_STEPS: i64 = 1_000_000;
const MAX_SLEEP_S: i64 = 24 * 60 * 60;

#[derive(Serialize, Deserialize, Validate, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Consents {
    pub steps: bool,
    pub workouts: bool,
    pub sleep: bool,
}

async fn load_consents(db: &mut PgConnection, user: Uuid) -> Result<Consents, AppError> {
    let rows: Vec<(String, bool)> =
        sqlx::query_as("SELECT category, granted FROM health_consents WHERE user_id = $1")
            .bind(user)
            .fetch_all(db)
            .await?;
    let mut c = Consents {
        steps: false,
        workouts: false,
        sleep: false,
    };
    for (cat, granted) in rows {
        match cat.as_str() {
            "steps" => c.steps = granted,
            "workouts" => c.workouts = granted,
            "sleep" => c.sleep = granted,
            _ => {}
        }
    }
    Ok(c)
}

async fn get_consents(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Consents>, AppError> {
    let mut conn = state.pool.acquire().await?;
    Ok(Json(load_consents(&mut conn, user.0).await?))
}

async fn put_consents(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<Consents>,
) -> Result<Json<Consents>, AppError> {
    let mut tx = state.pool.begin().await?;
    for (cat, granted) in [
        ("steps", req.steps),
        ("workouts", req.workouts),
        ("sleep", req.sleep),
    ] {
        sqlx::query(
            "INSERT INTO health_consents (user_id, category, granted) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, category) DO UPDATE SET granted = EXCLUDED.granted, \
                updated_at = now()",
        )
        .bind(user.0)
        .bind(cat)
        .bind(granted)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(req))
}

// ---------- import ----------

#[derive(Deserialize)]
pub struct StepItem {
    pub external_id: String,
    pub date: NaiveDate,
    pub count: i64,
}

#[derive(Deserialize)]
pub struct WorkoutItem {
    pub external_id: String,
    pub started_at: DateTime<Utc>,
    pub duration_s: i64,
    pub kind: String,
}

#[derive(Deserialize)]
pub struct SleepItem {
    pub external_id: String,
    pub bedtime: DateTime<Utc>,
    pub wake_time: DateTime<Utc>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ImportRequest {
    pub steps: Vec<StepItem>,
    pub workouts: Vec<WorkoutItem>,
    pub sleep: Vec<SleepItem>,
}

#[derive(Serialize, Default, Debug, PartialEq, Eq)]
pub struct Counts {
    pub imported: usize,
    pub skipped: usize,
    pub duplicates: usize,
}

#[derive(Serialize)]
pub struct ImportResponse {
    pub steps: Counts,
    pub workouts: Counts,
    pub sleep: Counts,
}

type Fields = BTreeMap<String, Vec<String>>;

fn bad(f: &mut Fields, key: String, code: &str) {
    f.entry(key).or_default().push(code.to_string());
}

fn check_id(f: &mut Fields, prefix: &str, i: usize, id: &str) {
    let n = id.chars().count();
    if !(1..=200).contains(&n) {
        bad(f, format!("{prefix}[{i}].external_id"), "length");
    }
}

fn validate_request(req: &ImportRequest, now: DateTime<Utc>) -> Result<(), AppError> {
    let mut f = Fields::new();
    for (name, len) in [
        ("steps", req.steps.len()),
        ("workouts", req.workouts.len()),
        ("sleep", req.sleep.len()),
    ] {
        if len > MAX_ITEMS {
            bad(&mut f, name.into(), "too_many");
        }
    }
    if !f.is_empty() {
        return Err(AppError::Validation(f));
    }
    let limit = now + Duration::days(1);
    let date_limit = limit.date_naive();
    for (i, s) in req.steps.iter().enumerate() {
        check_id(&mut f, "steps", i, &s.external_id);
        if !(0..=MAX_STEPS).contains(&s.count) {
            bad(&mut f, format!("steps[{i}].count"), "range");
        }
        if s.date > date_limit {
            bad(&mut f, format!("steps[{i}].date"), "in_future");
        }
    }
    for (i, w) in req.workouts.iter().enumerate() {
        check_id(&mut f, "workouts", i, &w.external_id);
        if !(1..=86400).contains(&w.duration_s) {
            bad(&mut f, format!("workouts[{i}].duration_s"), "range");
        }
        if w.started_at > limit {
            bad(&mut f, format!("workouts[{i}].started_at"), "in_future");
        }
        let k = w.kind.trim().chars().count();
        if !(1..=40).contains(&k) {
            bad(&mut f, format!("workouts[{i}].kind"), "length");
        }
    }
    for (i, s) in req.sleep.iter().enumerate() {
        check_id(&mut f, "sleep", i, &s.external_id);
        if s.wake_time <= s.bedtime {
            bad(&mut f, format!("sleep[{i}].wake_time"), "before_bedtime");
        } else if (s.wake_time - s.bedtime).num_seconds() > MAX_SLEEP_S {
            bad(&mut f, format!("sleep[{i}].wake_time"), "too_long");
        }
        if s.wake_time > limit {
            bad(&mut f, format!("sleep[{i}].wake_time"), "in_future");
        }
    }
    if f.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(f))
    }
}

fn tally(counts: &mut Counts, affected: u64) {
    if affected > 0 {
        counts.imported += 1;
    } else {
        counts.duplicates += 1;
    }
}

async fn import(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, AppError> {
    validate_request(&req, Utc::now())?;
    let mut tx = state.pool.begin().await?;
    let consents = load_consents(&mut tx, user.0).await?;
    let mut out = ImportResponse {
        steps: Counts::default(),
        workouts: Counts::default(),
        sleep: Counts::default(),
    };

    if consents.steps {
        for s in &req.steps {
            // Today's count grows during the day, so a changed value updates the
            // sample; an identical re-import is reported as a duplicate.
            let res = sqlx::query(
                "INSERT INTO health_samples (user_id, category, external_id, date, value) \
                 VALUES ($1, 'steps', $2, $3, $4) \
                 ON CONFLICT (user_id, category, external_id) DO UPDATE \
                    SET value = EXCLUDED.value, date = EXCLUDED.date \
                    WHERE health_samples.value IS DISTINCT FROM EXCLUDED.value \
                       OR health_samples.date IS DISTINCT FROM EXCLUDED.date",
            )
            .bind(user.0)
            .bind(&s.external_id)
            .bind(s.date)
            .bind(s.count)
            .execute(&mut *tx)
            .await?;
            tally(&mut out.steps, res.rows_affected());
        }
    } else {
        out.steps.skipped = req.steps.len();
    }

    if consents.workouts {
        for w in &req.workouts {
            let res = sqlx::query(
                "INSERT INTO workouts (user_id, started_at, duration_s, kind, source, external_id) \
                 VALUES ($1, $2, $3, $4, 'health', $5) \
                 ON CONFLICT (user_id, source, external_id) WHERE external_id IS NOT NULL \
                 DO NOTHING",
            )
            .bind(user.0)
            .bind(w.started_at)
            .bind(w.duration_s as i32)
            .bind(w.kind.trim())
            .bind(&w.external_id)
            .execute(&mut *tx)
            .await?;
            tally(&mut out.workouts, res.rows_affected());
        }
    } else {
        out.workouts.skipped = req.workouts.len();
    }

    if consents.sleep {
        for s in &req.sleep {
            let res = sqlx::query(
                "INSERT INTO sleep_logs (user_id, bedtime, wake_time, source, external_id) \
                 VALUES ($1, $2, $3, 'health', $4) \
                 ON CONFLICT (user_id, source, external_id) WHERE external_id IS NOT NULL \
                 DO NOTHING",
            )
            .bind(user.0)
            .bind(s.bedtime)
            .bind(s.wake_time)
            .bind(&s.external_id)
            .execute(&mut *tx)
            .await?;
            tally(&mut out.sleep, res.rows_affected());
        }
    } else {
        out.sleep.skipped = req.sleep.len();
    }

    tx.commit().await?;
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_items() {
        let now = Utc::now();
        let ok = ImportRequest {
            steps: vec![StepItem {
                external_id: "a".into(),
                date: now.date_naive(),
                count: 10,
            }],
            ..Default::default()
        };
        assert!(validate_request(&ok, now).is_ok());
        let bad_req = ImportRequest {
            steps: vec![StepItem {
                external_id: "".into(),
                date: now.date_naive(),
                count: -1,
            }],
            sleep: vec![SleepItem {
                external_id: "s".into(),
                bedtime: now,
                wake_time: now - Duration::hours(1),
            }],
            ..Default::default()
        };
        assert!(validate_request(&bad_req, now).is_err());
    }
}

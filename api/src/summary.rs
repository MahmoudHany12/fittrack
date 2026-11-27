//! Weekly summary: training, sleep, weight and diet for a 7-day UTC window,
//! compared with the previous week. Every number is `null` when there is no data.
use crate::{auth::AuthUser, error::AppError, AppState};
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/summary/weekly", get(weekly))
}

/// Weight changes smaller than this (kg) over the week read as "flat".
const FLAT_KG: f64 = 0.2;

#[derive(Debug, Serialize, PartialEq)]
pub struct Training {
    pub workouts: Option<i64>,
    pub minutes: Option<i64>,
    pub active_days: Option<i64>,
    pub vs_prev_week_minutes: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Sleep {
    pub avg_minutes: Option<i64>,
    pub avg_quality: Option<f64>,
    pub nights: Option<i64>,
    pub vs_prev_week_minutes: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Weight {
    pub start_kg: Option<f64>,
    pub end_kg: Option<f64>,
    pub change_kg: Option<f64>,
    /// "up" | "flat" | "down"
    pub trend: Option<&'static str>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Diet {
    pub avg_kcal: Option<i64>,
    pub days_logged: Option<i64>,
    pub target: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct WeeklySummary {
    /// First day of the window (inclusive).
    pub from: NaiveDate,
    /// Last day of the window (inclusive).
    pub to: NaiveDate,
    pub training: Training,
    pub sleep: Sleep,
    pub weight: Weight,
    pub diet: Diet,
}

#[derive(Deserialize)]
struct WeeklyQuery {
    week_start: Option<String>,
}

fn invalid(field: &str, code: &str) -> AppError {
    AppError::Validation(BTreeMap::from([(
        field.to_string(),
        vec![code.to_string()],
    )]))
}

async fn weekly(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<WeeklyQuery>,
) -> Result<Json<WeeklySummary>, AppError> {
    let start = match q.week_start.as_deref() {
        None => Utc::now().date_naive() - Duration::days(6),
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| invalid("week_start", "invalid_date"))?;
            if !(2000..=2100).contains(&d.year()) {
                return Err(invalid("week_start", "out_of_range"));
            }
            d
        }
    };
    Ok(Json(weekly_summary(&state.pool, user.0, start).await?))
}

fn day_start(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0).expect("midnight is valid").and_utc()
}

fn round(x: f64) -> i64 {
    x.round() as i64
}

/// Summary for the 7 days starting at `week_start` (UTC), scoped to `user_id`.
pub async fn weekly_summary(
    pool: &PgPool,
    user_id: Uuid,
    week_start: NaiveDate,
) -> Result<WeeklySummary, AppError> {
    let from = day_start(week_start);
    let to = from + Duration::days(7);
    let prev_from = from - Duration::days(7);

    let (workouts, minutes, active_days) = training_window(pool, user_id, from, to).await?;
    let (_, prev_minutes, _) = training_window(pool, user_id, prev_from, from).await?;
    let (nights, avg_sleep, avg_quality) = sleep_window(pool, user_id, from, to).await?;
    let (_, prev_avg_sleep, _) = sleep_window(pool, user_id, prev_from, from).await?;

    let weights: Vec<(f64,)> = sqlx::query_as(
        "SELECT kg::float8 FROM weight_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND measured_at >= $2 AND measured_at < $3 \
         ORDER BY measured_at, created_at",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    let weight = match (weights.first(), weights.last()) {
        (Some(&(s,)), Some(&(e,))) => {
            let change = ((e - s) * 100.0).round() / 100.0;
            Weight {
                start_kg: Some(s),
                end_kg: Some(e),
                change_kg: Some(change),
                trend: Some(if change > FLAT_KG {
                    "up"
                } else if change < -FLAT_KG {
                    "down"
                } else {
                    "flat"
                }),
            }
        }
        _ => Weight {
            start_kg: None,
            end_kg: None,
            change_kg: None,
            trend: None,
        },
    };

    let (kcal_total, days_logged): (Option<f64>, i64) = sqlx::query_as(
        "SELECT SUM(kcal)::float8, COUNT(DISTINCT (logged_at AT TIME ZONE 'UTC')::date) \
         FROM food_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND logged_at >= $2 AND logged_at < $3",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    let target: Option<(i32,)> =
        sqlx::query_as("SELECT calorie_target FROM profiles WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let diet = Diet {
        avg_kcal: kcal_total
            .filter(|_| days_logged > 0)
            .map(|k| round(k / days_logged as f64)),
        days_logged: (days_logged > 0).then_some(days_logged),
        target: target.map(|(t,)| i64::from(t)),
    };

    Ok(WeeklySummary {
        from: week_start,
        to: week_start + Duration::days(6),
        training: Training {
            workouts,
            minutes,
            active_days,
            vs_prev_week_minutes: minutes.zip(prev_minutes).map(|(a, b)| a - b),
        },
        sleep: Sleep {
            avg_minutes: avg_sleep,
            avg_quality,
            nights,
            vs_prev_week_minutes: avg_sleep.zip(prev_avg_sleep).map(|(a, b)| a - b),
        },
        weight,
        diet,
    })
}

type TrainingRow = (Option<i64>, Option<i64>, Option<i64>);

/// (workouts, minutes, active days) in `[from, to)`; all `None` when no workouts.
async fn training_window(
    pool: &PgPool,
    user_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<TrainingRow, AppError> {
    let (n, secs, days): (i64, Option<i64>, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(duration_s)::bigint, \
                COUNT(DISTINCT (started_at AT TIME ZONE 'UTC')::date) \
         FROM workouts \
         WHERE user_id = $1 AND deleted_at IS NULL AND started_at >= $2 AND started_at < $3",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    if n == 0 {
        return Ok((None, None, None));
    }
    let minutes = round(secs.unwrap_or(0) as f64 / 60.0);
    Ok((Some(n), Some(minutes), Some(days)))
}

type SleepRow = (Option<i64>, Option<i64>, Option<f64>);

/// (nights, average minutes, average quality) for nights ending in `[from, to)`.
async fn sleep_window(
    pool: &PgPool,
    user_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<SleepRow, AppError> {
    let (n, avg_secs, avg_q): (i64, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*), AVG(EXTRACT(EPOCH FROM (wake_time - bedtime)))::float8, \
                AVG(quality)::float8 \
         FROM sleep_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND wake_time >= $2 AND wake_time < $3",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    if n == 0 {
        return Ok((None, None, None));
    }
    Ok((
        Some(n),
        avg_secs.map(|s| round(s / 60.0)),
        avg_q.map(|q| (q * 10.0).round() / 10.0),
    ))
}

//! `GET /dashboard`: one call powering the Home screen (see docs/spec.md).
//! Every section degrades to nulls when there is no data; the endpoint never errors on empty accounts.

use crate::{
    auth::AuthUser,
    error::AppError,
    readiness::{self, Readiness, SleepInput, Weights, WorkoutInput},
    sleep, weight, AppState,
};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/dashboard", get(dashboard))
}

#[derive(Serialize, Debug, PartialEq)]
pub struct StatusOut {
    pub key: readiness::Status,
    pub label: &'static str,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct SleepOut {
    pub minutes: Option<i32>,
    pub quality: Option<i16>,
    pub label: Option<&'static str>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct DietOut {
    pub kcal_eaten: i64,
    pub kcal_target: i32,
    pub label: &'static str,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ExerciseOut {
    pub steps: Option<i64>,
    pub workout_minutes_today: i64,
    pub workout_count_today: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct WeightOut {
    pub kg: Option<f64>,
    pub trend: weight::Trend,
}

#[derive(Serialize)]
pub struct Dashboard {
    pub name: String,
    pub status: StatusOut,
    pub readiness: Readiness,
    pub sleep: SleepOut,
    pub diet: DietOut,
    pub exercise: ExerciseOut,
    pub weight: WeightOut,
    /// `null` unless the `stats` feature flag is on and the user has activity.
    pub stats: Option<serde_json::Value>,
}

/// Plain-language diet status. `hour` is the current UTC hour: "Under-eating" is
/// only reported in the evening so a normal morning does not look like a problem.
pub fn diet_label(eaten: i64, target: i32, hour: u32) -> &'static str {
    let target = i64::from(target.max(1));
    if eaten * 100 > target * 110 {
        "Overeating"
    } else if hour >= 20 && eaten * 100 < target * 70 {
        "Under-eating"
    } else {
        "On track"
    }
}

fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.from_utc_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default())
}

async fn dashboard(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Dashboard>, AppError> {
    let now = Utc::now();
    Ok(Json(build(&state.pool, user.0, now).await?))
}

pub async fn build(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Dashboard, AppError> {
    let profile: Option<(Option<String>, i32)> =
        sqlx::query_as("SELECT display_name, calorie_target FROM profiles WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let (display_name, calorie_target) = profile.unwrap_or((None, 2200));
    let name = match display_name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n,
        None => {
            let email: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(pool)
                .await?;
            email
                .map(|(e,)| e.split('@').next().unwrap_or("").to_string())
                .unwrap_or_default()
        }
    };

    let last_sleep = sleep::last_night(pool, user_id, now).await?;
    let sleep_input = last_sleep.as_ref().map(|s| SleepInput {
        minutes: i64::from(s.minutes),
        quality: s.quality,
    });

    let rows: Vec<(DateTime<Utc>, i32)> = sqlx::query_as(
        "SELECT started_at, duration_s FROM workouts \
         WHERE user_id = $1 AND deleted_at IS NULL AND started_at > $2 AND started_at <= $3",
    )
    .bind(user_id)
    .bind(now - Duration::days(8))
    .bind(now)
    .fetch_all(pool)
    .await?;
    let workouts: Vec<WorkoutInput> = rows
        .iter()
        .map(|(t, d)| WorkoutInput {
            started_at: *t,
            duration_s: i64::from(*d),
        })
        .collect();

    let day_start = start_of_day(now);
    let (count_today, secs_today): (i64, i64) = {
        let today: Vec<_> = rows.iter().filter(|(t, _)| *t >= day_start).collect();
        (
            today.len() as i64,
            today.iter().map(|(_, d)| i64::from(*d)).sum(),
        )
    };

    let kcal: (Option<f64>,) = sqlx::query_as(
        "SELECT SUM(kcal)::float8 FROM food_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND logged_at >= $2 AND logged_at < $3",
    )
    .bind(user_id)
    .bind(day_start)
    .bind(day_start + Duration::days(1))
    .fetch_one(pool)
    .await?;
    let kcal_eaten = kcal.0.unwrap_or(0.0).round() as i64;

    let ready = readiness::compute(sleep_input.as_ref(), &workouts, now, Weights::from_env());
    let trend = weight::weight_trend(pool, user_id, now).await?;

    let sleep_label = sleep_input.as_ref().map(|s| {
        readiness::Status::from_score(Some(readiness::sleep_score(s).round() as u8)).label()
    });

    Ok(Dashboard {
        name,
        status: StatusOut {
            key: ready.status,
            label: ready.status.label(),
        },
        readiness: ready,
        sleep: SleepOut {
            minutes: last_sleep.as_ref().map(|s| s.minutes),
            quality: last_sleep.as_ref().and_then(|s| s.quality),
            label: sleep_label,
        },
        diet: DietOut {
            kcal_eaten,
            kcal_target: calorie_target,
            label: diet_label(kcal_eaten, calorie_target, now.hour()),
        },
        exercise: ExerciseOut {
            steps: None, // wired to health samples once health import lands
            workout_minutes_today: secs_today / 60,
            workout_count_today: count_today,
        },
        weight: WeightOut {
            kg: trend.current_kg,
            trend: trend.trend,
        },
        stats: None, // wired to stats::current_stats once the stats feature lands
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diet_labels() {
        assert_eq!(diet_label(2000, 2200, 12), "On track");
        assert_eq!(diet_label(2500, 2200, 12), "Overeating");
        assert_eq!(diet_label(2420, 2200, 12), "On track"); // exactly 110%
        assert_eq!(diet_label(500, 2200, 9), "On track"); // morning, not a problem yet
        assert_eq!(diet_label(500, 2200, 21), "Under-eating");
        assert_eq!(diet_label(1600, 2200, 21), "On track"); // 72%
        assert_eq!(diet_label(0, 0, 21), "Under-eating"); // zero target does not divide by zero
    }
}

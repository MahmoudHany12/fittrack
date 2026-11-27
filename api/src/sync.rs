//! Offline sync: `POST /sync` (push local changes, pull server changes).
//!
//! Conflict policy is **last write wins** on the client-supplied `updated_at`
//! (clamped to server time so a skewed clock cannot win forever). An incoming
//! change is applied only if it is strictly newer than the stored row; ties keep
//! the server copy. Deletes are soft-deletes and follow the same rule, so a newer
//! edit after a delete resurrects the row. Rows owned by another user are never
//! touched: they count as ignored.
//!
//! Pulling uses the database-maintained `synced_at` cursor (see migration 0007),
//! not `updated_at`, so a late-arriving offline edit is still delivered to other
//! devices. Clients should send `since = previous server_time - 5s` to tolerate
//! transactions that commit slightly out of order; re-delivered rows are harmless.

use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{extract::State, routing::post, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;
use validator::Validate;

const MAX_PUSH_PER_TABLE: usize = 500;
const MAX_SETS: usize = 100;
const PULL_LIMIT: i64 = 1000;

pub fn routes() -> Router<AppState> {
    Router::new().route("/sync", post(sync))
}

// ---------- request ----------

#[derive(Deserialize, Validate)]
pub struct SyncRequest {
    pub since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub changes: Changes,
}

#[derive(Deserialize, Default)]
pub struct Changes {
    #[serde(default)]
    pub workouts: Vec<WorkoutChange>,
    #[serde(default)]
    pub sleep: Vec<SleepChange>,
    #[serde(default)]
    pub weight: Vec<WeightChange>,
    #[serde(default)]
    pub food_logs: Vec<FoodLogChange>,
}

#[derive(Deserialize)]
pub struct SetIn {
    pub exercise_id: Uuid,
    pub reps: i32,
    pub weight_kg: f64,
}

#[derive(Deserialize)]
pub struct WorkoutChange {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub duration_s: Option<i32>,
    pub note: Option<String>,
    #[serde(default)]
    pub sets: Vec<SetIn>,
}

#[derive(Deserialize)]
pub struct SleepChange {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted: bool,
    pub bedtime: Option<DateTime<Utc>>,
    pub wake_time: Option<DateTime<Utc>>,
    pub quality: Option<i16>,
}

#[derive(Deserialize)]
pub struct WeightChange {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted: bool,
    pub measured_at: Option<DateTime<Utc>>,
    pub kg: Option<f64>,
}

#[derive(Deserialize)]
pub struct FoodLogChange {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted: bool,
    pub food_id: Option<Uuid>,
    pub logged_at: Option<DateTime<Utc>>,
    pub servings: Option<f64>,
    pub kcal: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbs_g: Option<f64>,
    pub fat_g: Option<f64>,
    pub meal: Option<String>,
}

// ---------- response ----------

#[derive(Serialize, Default)]
pub struct TableResult {
    pub applied: u32,
    pub ignored: u32,
}

#[derive(Serialize)]
pub struct Rejected {
    pub table: &'static str,
    pub id: Uuid,
    pub reason: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SetOut {
    pub exercise_id: Uuid,
    pub reps: i32,
    pub weight_kg: f64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkoutOut {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub duration_s: i32,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub synced_at: DateTime<Utc>,
    #[sqlx(skip)]
    pub sets: Vec<SetOut>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SleepOut {
    pub id: Uuid,
    pub bedtime: DateTime<Utc>,
    pub wake_time: DateTime<Utc>,
    pub quality: Option<i16>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub synced_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WeightOut {
    pub id: Uuid,
    pub measured_at: DateTime<Utc>,
    pub kg: f64,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub synced_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct FoodLogOut {
    pub id: Uuid,
    pub food_id: Uuid,
    pub logged_at: DateTime<Utc>,
    pub servings: f64,
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub meal: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub synced_at: DateTime<Utc>,
}

#[derive(Serialize, Default)]
pub struct Pulled {
    pub workouts: Vec<WorkoutOut>,
    pub sleep: Vec<SleepOut>,
    pub weight: Vec<WeightOut>,
    pub food_logs: Vec<FoodLogOut>,
}

#[derive(Serialize)]
pub struct SyncResponse {
    /// Store this and send `server_time - 5s` as `since` next time.
    pub server_time: DateTime<Utc>,
    /// True when a table was truncated; call again with `since = server_time` (no overlap).
    pub has_more: bool,
    pub applied: BTreeMap<&'static str, TableResult>,
    pub rejected: Vec<Rejected>,
    pub changes: Pulled,
}

// ---------- validation helpers ----------

type Reject = Result<(), String>;

fn check_finite(v: f64, name: &str) -> Reject {
    if v.is_finite() {
        Ok(())
    } else {
        Err(format!("{name} must be a number"))
    }
}

fn required<T: Copy>(v: Option<T>, name: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("{name} is required"))
}

fn clamp_ts(ts: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    ts.min(now)
}

// ---------- handler ----------

async fn sync(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<SyncRequest>,
) -> Result<Json<SyncResponse>, AppError> {
    let c = &req.changes;
    if c.workouts.len() > MAX_PUSH_PER_TABLE
        || c.sleep.len() > MAX_PUSH_PER_TABLE
        || c.weight.len() > MAX_PUSH_PER_TABLE
        || c.food_logs.len() > MAX_PUSH_PER_TABLE
    {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_PUSH_PER_TABLE} changes per table per request"
        )));
    }

    let mut tx = state.pool.begin().await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let mut applied: BTreeMap<&'static str, TableResult> = BTreeMap::new();
    let mut rejected = Vec::new();

    for ch in &c.workouts {
        record(
            "workouts",
            ch.id,
            apply_workout(&mut tx, user.0, ch, now).await?,
            &mut applied,
            &mut rejected,
        );
    }
    for ch in &c.sleep {
        record(
            "sleep",
            ch.id,
            apply_sleep(&mut tx, user.0, ch, now).await?,
            &mut applied,
            &mut rejected,
        );
    }
    for ch in &c.weight {
        record(
            "weight",
            ch.id,
            apply_weight(&mut tx, user.0, ch, now).await?,
            &mut applied,
            &mut rejected,
        );
    }
    for ch in &c.food_logs {
        record(
            "food_logs",
            ch.id,
            apply_food_log(&mut tx, user.0, ch, now).await?,
            &mut applied,
            &mut rejected,
        );
    }
    tx.commit().await?;

    for t in ["workouts", "sleep", "weight", "food_logs"] {
        applied.entry(t).or_default();
    }

    let (changes, server_time, has_more) = pull(&state.pool, user.0, req.since).await?;
    Ok(Json(SyncResponse {
        server_time,
        has_more,
        applied,
        rejected,
        changes,
    }))
}

fn record(
    table: &'static str,
    id: Uuid,
    outcome: Result<bool, String>,
    applied: &mut BTreeMap<&'static str, TableResult>,
    rejected: &mut Vec<Rejected>,
) {
    let entry = applied.entry(table).or_default();
    match outcome {
        Ok(true) => entry.applied += 1,
        Ok(false) => entry.ignored += 1,
        Err(reason) => rejected.push(Rejected { table, id, reason }),
    }
}

// ---------- apply (push) ----------
// Each returns Ok(Ok(true)) applied, Ok(Ok(false)) ignored (stale/foreign/unknown delete),
// Ok(Err(reason)) rejected for validation, Err(AppError) for database failures.

async fn soft_delete(
    conn: &mut PgConnection,
    table: &str,
    id: Uuid,
    user_id: Uuid,
    ts: DateTime<Utc>,
) -> Result<bool, AppError> {
    // `table` is always one of our own string literals.
    let sql = format!(
        "UPDATE {table} SET deleted_at = $3, updated_at = $3 \
         WHERE id = $1 AND user_id = $2 AND updated_at < $3"
    );
    let res = sqlx::query(&sql)
        .bind(id)
        .bind(user_id)
        .bind(ts)
        .execute(conn)
        .await?;
    Ok(res.rows_affected() > 0)
}

async fn apply_workout(
    conn: &mut PgConnection,
    user_id: Uuid,
    ch: &WorkoutChange,
    now: DateTime<Utc>,
) -> Result<Result<bool, String>, AppError> {
    let ts = clamp_ts(ch.updated_at, now);
    if ch.deleted {
        return Ok(Ok(soft_delete(conn, "workouts", ch.id, user_id, ts).await?));
    }
    let checked = (|| -> Result<(DateTime<Utc>, i32), String> {
        let started_at = required(ch.started_at, "started_at")?;
        let duration = required(ch.duration_s, "duration_s")?;
        if !(1..=86400).contains(&duration) {
            return Err("duration_s must be 1..86400".into());
        }
        if ch.note.as_ref().is_some_and(|n| n.chars().count() > 500) {
            return Err("note is too long".into());
        }
        if ch.sets.len() > MAX_SETS {
            return Err(format!("at most {MAX_SETS} sets"));
        }
        for s in &ch.sets {
            if !(1..=1000).contains(&s.reps) {
                return Err("reps must be 1..1000".into());
            }
            check_finite(s.weight_kg, "weight_kg")?;
            if !(0.0..=1000.0).contains(&s.weight_kg) {
                return Err("weight_kg must be 0..1000".into());
            }
        }
        Ok((started_at, duration))
    })();
    let (started_at, duration) = match checked {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    // Exercises must be catalog entries or this user's own.
    let mut ids: Vec<Uuid> = ch.sets.iter().map(|s| s.exercise_id).collect();
    ids.sort();
    ids.dedup();
    if !ids.is_empty() {
        let ok: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM exercises WHERE id = ANY($1) AND (owner_id IS NULL OR owner_id = $2)",
        )
        .bind(&ids)
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await?;
        if ok != ids.len() as i64 {
            return Ok(Err("unknown exercise_id".into()));
        }
    }
    let won: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO workouts (id, user_id, started_at, duration_s, note, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (id) DO UPDATE SET started_at = EXCLUDED.started_at, \
             duration_s = EXCLUDED.duration_s, note = EXCLUDED.note, \
             updated_at = EXCLUDED.updated_at, deleted_at = NULL \
         WHERE workouts.user_id = EXCLUDED.user_id AND workouts.updated_at < EXCLUDED.updated_at \
         RETURNING id",
    )
    .bind(ch.id)
    .bind(user_id)
    .bind(started_at)
    .bind(duration)
    .bind(&ch.note)
    .bind(ts)
    .fetch_optional(&mut *conn)
    .await?;
    if won.is_none() {
        return Ok(Ok(false));
    }
    sqlx::query("DELETE FROM workout_sets WHERE workout_id = $1")
        .bind(ch.id)
        .execute(&mut *conn)
        .await?;
    for (pos, s) in ch.sets.iter().enumerate() {
        sqlx::query(
            "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(ch.id)
        .bind(s.exercise_id)
        .bind(s.reps)
        .bind(s.weight_kg)
        .bind(pos as i32)
        .execute(&mut *conn)
        .await?;
    }
    Ok(Ok(true))
}

async fn apply_sleep(
    conn: &mut PgConnection,
    user_id: Uuid,
    ch: &SleepChange,
    now: DateTime<Utc>,
) -> Result<Result<bool, String>, AppError> {
    let ts = clamp_ts(ch.updated_at, now);
    if ch.deleted {
        return Ok(Ok(
            soft_delete(conn, "sleep_logs", ch.id, user_id, ts).await?
        ));
    }
    let checked = (|| -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
        let bed = required(ch.bedtime, "bedtime")?;
        let wake = required(ch.wake_time, "wake_time")?;
        if wake <= bed {
            return Err("wake_time must be after bedtime".into());
        }
        if wake - bed > Duration::hours(24) {
            return Err("sleep cannot be longer than 24 hours".into());
        }
        if wake > now + Duration::days(1) {
            return Err("wake_time is too far in the future".into());
        }
        if ch.quality.is_some_and(|q| !(1..=5).contains(&q)) {
            return Err("quality must be 1..5".into());
        }
        Ok((bed, wake))
    })();
    let (bed, wake) = match checked {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    let won: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO sleep_logs (id, user_id, bedtime, wake_time, quality, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (id) DO UPDATE SET bedtime = EXCLUDED.bedtime, wake_time = EXCLUDED.wake_time, \
             quality = EXCLUDED.quality, updated_at = EXCLUDED.updated_at, deleted_at = NULL \
         WHERE sleep_logs.user_id = EXCLUDED.user_id AND sleep_logs.updated_at < EXCLUDED.updated_at \
         RETURNING id",
    )
    .bind(ch.id)
    .bind(user_id)
    .bind(bed)
    .bind(wake)
    .bind(ch.quality)
    .bind(ts)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Ok(won.is_some()))
}

async fn apply_weight(
    conn: &mut PgConnection,
    user_id: Uuid,
    ch: &WeightChange,
    now: DateTime<Utc>,
) -> Result<Result<bool, String>, AppError> {
    let ts = clamp_ts(ch.updated_at, now);
    if ch.deleted {
        return Ok(Ok(
            soft_delete(conn, "weight_logs", ch.id, user_id, ts).await?
        ));
    }
    let checked = (|| -> Result<(DateTime<Utc>, f64), String> {
        let at = required(ch.measured_at, "measured_at")?;
        let kg = required(ch.kg, "kg")?;
        check_finite(kg, "kg")?;
        if !(20.0..=400.0).contains(&kg) {
            return Err("kg must be 20..400".into());
        }
        Ok((at, kg))
    })();
    let (at, kg) = match checked {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    let won: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO weight_logs (id, user_id, measured_at, kg, updated_at) \
         VALUES ($1, $2, $3, $4::float8, $5) \
         ON CONFLICT (id) DO UPDATE SET measured_at = EXCLUDED.measured_at, kg = EXCLUDED.kg, \
             updated_at = EXCLUDED.updated_at, deleted_at = NULL \
         WHERE weight_logs.user_id = EXCLUDED.user_id AND weight_logs.updated_at < EXCLUDED.updated_at \
         RETURNING id",
    )
    .bind(ch.id)
    .bind(user_id)
    .bind(at)
    .bind(kg)
    .bind(ts)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Ok(won.is_some()))
}

async fn apply_food_log(
    conn: &mut PgConnection,
    user_id: Uuid,
    ch: &FoodLogChange,
    now: DateTime<Utc>,
) -> Result<Result<bool, String>, AppError> {
    let ts = clamp_ts(ch.updated_at, now);
    if ch.deleted {
        return Ok(Ok(soft_delete(conn, "food_logs", ch.id, user_id, ts).await?));
    }
    #[allow(clippy::type_complexity)]
    let checked = (|| -> Result<(Uuid, DateTime<Utc>, f64, [f64; 4]), String> {
        let food_id = required(ch.food_id, "food_id")?;
        let logged_at = required(ch.logged_at, "logged_at")?;
        let servings = required(ch.servings, "servings")?;
        let macros = [
            required(ch.kcal, "kcal")?,
            required(ch.protein_g, "protein_g")?,
            required(ch.carbs_g, "carbs_g")?,
            required(ch.fat_g, "fat_g")?,
        ];
        check_finite(servings, "servings")?;
        if servings <= 0.0 || servings > 1000.0 {
            return Err("servings must be > 0 and <= 1000".into());
        }
        for m in macros {
            check_finite(m, "nutrient")?;
            if !(0.0..=100_000.0).contains(&m) {
                return Err("nutrient values must be 0..100000".into());
            }
        }
        if let Some(meal) = &ch.meal {
            if !["breakfast", "lunch", "dinner", "snack"].contains(&meal.as_str()) {
                return Err("meal must be breakfast, lunch, dinner or snack".into());
            }
        }
        Ok((food_id, logged_at, servings, macros))
    })();
    let (food_id, logged_at, servings, m) = match checked {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    let visible: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM foods WHERE id = $1 AND (owner_id IS NULL OR owner_id = $2)",
    )
    .bind(food_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    if visible.is_none() {
        return Ok(Err("unknown food_id".into()));
    }
    let won: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g, meal, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (id) DO UPDATE SET food_id = EXCLUDED.food_id, logged_at = EXCLUDED.logged_at, \
             servings = EXCLUDED.servings, kcal = EXCLUDED.kcal, protein_g = EXCLUDED.protein_g, \
             carbs_g = EXCLUDED.carbs_g, fat_g = EXCLUDED.fat_g, meal = EXCLUDED.meal, \
             updated_at = EXCLUDED.updated_at, deleted_at = NULL \
         WHERE food_logs.user_id = EXCLUDED.user_id AND food_logs.updated_at < EXCLUDED.updated_at \
         RETURNING id",
    )
    .bind(ch.id)
    .bind(user_id)
    .bind(food_id)
    .bind(logged_at)
    .bind(servings)
    .bind(m[0])
    .bind(m[1])
    .bind(m[2])
    .bind(m[3])
    .bind(&ch.meal)
    .bind(ts)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Ok(won.is_some()))
}

// ---------- pull ----------

async fn pull(
    pool: &PgPool,
    user_id: Uuid,
    since: Option<DateTime<Utc>>,
) -> Result<(Pulled, DateTime<Utc>, bool), AppError> {
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await?;
    // Fetch one extra row to detect truncation.
    let mut workouts: Vec<WorkoutOut> = sqlx::query_as(
        "SELECT id, started_at, duration_s, note, updated_at, (deleted_at IS NOT NULL) AS deleted, synced_at \
         FROM workouts WHERE user_id = $1 AND ($2::timestamptz IS NULL OR synced_at > $2) \
         ORDER BY synced_at LIMIT $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(PULL_LIMIT + 1)
    .fetch_all(pool)
    .await?;
    let mut sleep: Vec<SleepOut> = sqlx::query_as(
        "SELECT id, bedtime, wake_time, quality, updated_at, (deleted_at IS NOT NULL) AS deleted, synced_at \
         FROM sleep_logs WHERE user_id = $1 AND ($2::timestamptz IS NULL OR synced_at > $2) \
         ORDER BY synced_at LIMIT $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(PULL_LIMIT + 1)
    .fetch_all(pool)
    .await?;
    let mut weight: Vec<WeightOut> = sqlx::query_as(
        "SELECT id, measured_at, kg::float8 AS kg, updated_at, (deleted_at IS NOT NULL) AS deleted, synced_at \
         FROM weight_logs WHERE user_id = $1 AND ($2::timestamptz IS NULL OR synced_at > $2) \
         ORDER BY synced_at LIMIT $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(PULL_LIMIT + 1)
    .fetch_all(pool)
    .await?;
    let mut food_logs: Vec<FoodLogOut> = sqlx::query_as(
        "SELECT id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g, meal, updated_at, \
                (deleted_at IS NOT NULL) AS deleted, synced_at \
         FROM food_logs WHERE user_id = $1 AND ($2::timestamptz IS NULL OR synced_at > $2) \
         ORDER BY synced_at LIMIT $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(PULL_LIMIT + 1)
    .fetch_all(pool)
    .await?;

    // Truncate each table; if any was cut, the cursor must stop at the earliest cut point.
    let limit = PULL_LIMIT as usize;
    let mut cursor = now;
    let mut has_more = false;
    macro_rules! cut {
        ($v:expr) => {
            if $v.len() > limit {
                $v.truncate(limit);
                has_more = true;
                if let Some(last) = $v.last() {
                    cursor = cursor.min(last.synced_at);
                }
            }
        };
    }
    cut!(workouts);
    cut!(sleep);
    cut!(weight);
    cut!(food_logs);

    if !workouts.is_empty() {
        let ids: Vec<Uuid> = workouts.iter().map(|w| w.id).collect();
        let rows: Vec<(Uuid, Uuid, i32, f64)> = sqlx::query_as(
            "SELECT workout_id, exercise_id, reps, weight_kg FROM workout_sets \
             WHERE workout_id = ANY($1) ORDER BY workout_id, position",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await?;
        let mut by: HashMap<Uuid, Vec<SetOut>> = HashMap::new();
        for (wid, exercise_id, reps, weight_kg) in rows {
            by.entry(wid).or_default().push(SetOut {
                exercise_id,
                reps,
                weight_kg,
            });
        }
        for w in &mut workouts {
            w.sets = by.remove(&w.id).unwrap_or_default();
        }
    }

    Ok((
        Pulled {
            workouts,
            sleep,
            weight,
            food_logs,
        },
        cursor,
        has_more,
    ))
}

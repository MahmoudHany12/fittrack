//! Game-style stats (Strength, Cardio, Focus). See docs/stats.md.
//!
//! `compute_stats` is a pure function; `current_stats` loads the user's recent
//! data, computes, and upserts the daily snapshot. Every query is scoped to
//! the given user id.

use crate::{auth::AuthUser, error::AppError, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/stats", get(get_stats))
}

pub const BASELINE: f64 = 10.0;
const WINDOW_DAYS: i64 = 28;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkoutInput {
    pub started_at: DateTime<Utc>,
    /// Sets of non-cardio exercises.
    pub strength_sets: u32,
    /// Minutes attributed to cardio-group exercises.
    pub cardio_minutes: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SleepInput {
    pub wake_time: DateTime<Utc>,
    pub minutes: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub workouts: Vec<WorkoutInput>,
    pub sleeps: Vec<SleepInput>,
    /// Latest body scan with skeletal muscle mass, if any.
    pub muscle: Option<MuscleInput>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MuscleInput {
    pub skeletal_muscle_kg: f64,
    pub weight_kg: f64,
    /// `None` when the profile has no sex; midpoint thresholds are used.
    pub male: Option<bool>,
}

/// Strength boost (0..=10) from skeletal-muscle-mass relative to body weight:
/// ratio >= 0.45 male / 0.38 female gives +10, scaled linearly from 0.35 / 0.28.
pub fn muscle_boost(m: &MuscleInput) -> f64 {
    if m.weight_kg <= 0.0 {
        return 0.0;
    }
    let (lo, hi) = match m.male {
        Some(true) => (0.35, 0.45),
        Some(false) => (0.28, 0.38),
        None => (0.315, 0.415),
    };
    let ratio = m.skeletal_muscle_kg / m.weight_kg;
    10.0 * ((ratio - lo) / (hi - lo)).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Stat {
    pub value: u8,
    pub level: u8,
    pub progress: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Stats {
    pub strength: Stat,
    pub cardio: Stat,
    pub focus: Stat,
}

impl Stats {
    /// All-zero stats (level 1, progress 0) for users with no activity yet.
    pub fn zero() -> Self {
        let z = make_stat(0.0);
        Stats {
            strength: z,
            cardio: z,
            focus: z,
        }
    }
}

/// Level 1-4 from a 0-100 value.
pub fn level_of(value: u8) -> u8 {
    match value {
        0..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        _ => 4,
    }
}

pub fn make_stat(value: f64) -> Stat {
    let v = value.round().clamp(0.0, 100.0) as u8;
    let level = level_of(v);
    let lo = f64::from((level - 1) * 25);
    Stat {
        value: v,
        level,
        progress: ((f64::from(v) - lo) / 25.0).clamp(0.0, 1.0),
    }
}

fn saturating(points: f64, scale: f64) -> f64 {
    BASELINE + (100.0 - BASELINE) * (1.0 - (-points.max(0.0) / scale).exp())
}

/// Pure stats computation; see docs/stats.md for the formulas.
pub fn compute_stats(inputs: &Inputs, now: DateTime<Utc>) -> Stats {
    let in_window = |t: DateTime<Utc>, days: i64| t <= now && now - t < Duration::days(days);

    // Strength / cardio over 28 days.
    let (mut sessions, mut sets, mut cardio_min) = (0.0, 0.0, 0.0);
    for w in inputs
        .workouts
        .iter()
        .filter(|w| in_window(w.started_at, WINDOW_DAYS))
    {
        if w.strength_sets > 0 {
            sessions += 1.0;
            sets += f64::from(w.strength_sets);
        }
        cardio_min += w.cardio_minutes.max(0.0);
    }
    let strength_raw = saturating(sessions * 6.0 + sets, 60.0);
    let cardio_raw = saturating(cardio_min, 300.0);

    // Focus over 14 days: nights >= 7h and distinct active days.
    let mut nights: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for s in inputs.sleeps.iter().filter(|s| in_window(s.wake_time, 14)) {
        *nights.entry(s.wake_time.date_naive()).or_default() += s.minutes.max(0.0);
    }
    let good_nights = nights.values().filter(|m| **m >= 420.0).count() as f64;
    let mut days: Vec<NaiveDate> = inputs
        .workouts
        .iter()
        .filter(|w| in_window(w.started_at, 14))
        .map(|w| w.started_at.date_naive())
        .collect();
    days.sort();
    days.dedup();
    let focus_raw = BASELINE
        + (100.0 - BASELINE)
            * (0.5 * (good_nights / 10.0).min(1.0) + 0.5 * (days.len() as f64 / 6.0).min(1.0));

    // Inactivity decay toward baseline: 0 days -> none, 14+ days -> baseline.
    let last = inputs
        .workouts
        .iter()
        .filter(|w| w.started_at <= now)
        .map(|w| w.started_at)
        .max();
    let keep = match last {
        None => 0.0,
        Some(t) => {
            let idle = ((now - t).num_seconds().max(0) as f64) / 86_400.0;
            (1.0 - idle / 14.0).clamp(0.0, 1.0)
        }
    };
    let decay = |raw: f64| BASELINE + (raw - BASELINE) * keep;

    // Sleep penalty: avg < 6h over last 7 nights, up to 20% at 4h.
    let recent: Vec<f64> = inputs
        .sleeps
        .iter()
        .filter(|s| in_window(s.wake_time, 7))
        .map(|s| s.minutes.max(0.0) / 60.0)
        .collect();
    let penalty = if recent.is_empty() {
        0.0
    } else {
        let avg = recent.iter().sum::<f64>() / recent.len() as f64;
        ((6.0 - avg) / 2.0).clamp(0.0, 1.0) * 0.2
    };
    let finish = |raw: f64| make_stat((decay(raw) * (1.0 - penalty)).clamp(0.0, 100.0));

    let boost = inputs.muscle.as_ref().map_or(0.0, muscle_boost);
    let strength = finish(strength_raw);
    Stats {
        strength: if boost > 0.0 {
            make_stat(f64::from(strength.value) + boost)
        } else {
            strength
        },
        cardio: finish(cardio_raw),
        focus: finish(focus_raw),
    }
}

/// Loads recent data, computes stats and upserts today's snapshot.
/// `None` when the user has no workouts or sleep in the last 28 days.
pub async fn current_stats(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<Stats>, sqlx::Error> {
    let since = now - Duration::days(WINDOW_DAYS);
    let rows: Vec<(DateTime<Utc>, i32, i64, i64)> = sqlx::query_as(
        "SELECT w.started_at, w.duration_s, \
                COUNT(s.id) FILTER (WHERE e.muscle_group <> 'cardio'), \
                COUNT(s.id) FILTER (WHERE e.muscle_group = 'cardio') \
         FROM workouts w \
         LEFT JOIN workout_sets s ON s.workout_id = w.id \
         LEFT JOIN exercises e ON e.id = s.exercise_id \
         WHERE w.user_id = $1 AND w.deleted_at IS NULL \
           AND w.started_at > $2 AND w.started_at <= $3 \
         GROUP BY w.id",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_all(pool)
    .await?;
    let workouts: Vec<WorkoutInput> = rows
        .into_iter()
        .map(|(started_at, duration_s, strength, cardio)| {
            let minutes = f64::from(duration_s.max(0)) / 60.0;
            let total = (strength + cardio) as f64;
            // No sets (e.g. imported from health) counts as cardio time.
            let cardio_minutes = if total == 0.0 {
                minutes
            } else {
                minutes * cardio as f64 / total
            };
            WorkoutInput {
                started_at,
                strength_sets: strength as u32,
                cardio_minutes,
            }
        })
        .collect();

    let sleep_rows: Vec<(DateTime<Utc>, f64)> = sqlx::query_as(
        "SELECT wake_time, (EXTRACT(EPOCH FROM (wake_time - bedtime)) / 60.0)::float8 \
         FROM sleep_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND wake_time > $2 AND wake_time <= $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_all(pool)
    .await?;
    let sleeps: Vec<SleepInput> = sleep_rows
        .into_iter()
        .map(|(wake_time, minutes)| SleepInput { wake_time, minutes })
        .collect();

    if workouts.is_empty() && sleeps.is_empty() {
        return Ok(None);
    }
    let scan: Option<(f64, f64)> = sqlx::query_as(
        "SELECT skeletal_muscle_kg, weight_kg FROM body_compositions \
         WHERE user_id = $1 AND deleted_at IS NULL AND skeletal_muscle_kg IS NOT NULL \
           AND measured_at <= $2 \
         ORDER BY measured_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let sex: Option<String> = sqlx::query_scalar("SELECT sex FROM profiles WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    let muscle = scan.map(|(skeletal_muscle_kg, weight_kg)| MuscleInput {
        skeletal_muscle_kg,
        weight_kg,
        male: sex.map(|s| s == "male"),
    });
    let inputs = Inputs {
        workouts,
        sleeps,
        muscle,
    };
    let stats = compute_stats(&inputs, now);
    sqlx::query(
        "INSERT INTO character_stats (user_id, day, strength, cardio, focus, computed_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (user_id, day) DO UPDATE SET strength = EXCLUDED.strength, \
           cardio = EXCLUDED.cardio, focus = EXCLUDED.focus, computed_at = EXCLUDED.computed_at",
    )
    .bind(user_id)
    .bind(now.date_naive())
    .bind(i16::from(stats.strength.value))
    .bind(i16::from(stats.cardio.value))
    .bind(i16::from(stats.focus.value))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(Some(stats))
}

/// Like [`current_stats`] but never `None`: users with no activity get [`Stats::zero`].
pub async fn current_stats_or_zero(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Stats, sqlx::Error> {
    Ok(current_stats(pool, user_id, now)
        .await?
        .unwrap_or_else(Stats::zero))
}

async fn get_stats(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Option<Stats>>, AppError> {
    if !state.config.feature("stats") {
        return Err(AppError::NotFound);
    }
    Ok(Json(current_stats(&state.pool, user.0, Utc::now()).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()
    }

    fn lift(days_ago: i64, sets: u32) -> WorkoutInput {
        WorkoutInput {
            started_at: now() - Duration::days(days_ago),
            strength_sets: sets,
            cardio_minutes: 0.0,
        }
    }

    fn run(days_ago: i64, minutes: f64) -> WorkoutInput {
        WorkoutInput {
            started_at: now() - Duration::days(days_ago),
            strength_sets: 0,
            cardio_minutes: minutes,
        }
    }

    fn night(days_ago: i64, hours: f64) -> SleepInput {
        SleepInput {
            wake_time: now() - Duration::days(days_ago) - Duration::hours(2),
            minutes: hours * 60.0,
        }
    }

    fn inputs(w: Vec<WorkoutInput>, s: Vec<SleepInput>) -> Inputs {
        Inputs {
            workouts: w,
            sleeps: s,
            muscle: None,
        }
    }

    #[test]
    fn zero_stats_are_level_one() {
        let z = Stats::zero();
        for s in [z.strength, z.cardio, z.focus] {
            assert_eq!((s.value, s.level), (0, 1));
            assert_eq!(s.progress, 0.0);
        }
    }

    #[test]
    fn muscle_boost_scales_by_sex() {
        let m = |smm, male| MuscleInput {
            skeletal_muscle_kg: smm,
            weight_kg: 100.0,
            male,
        };
        assert_eq!(muscle_boost(&m(45.0, Some(true))), 10.0);
        assert_eq!(muscle_boost(&m(50.0, Some(true))), 10.0);
        assert_eq!(muscle_boost(&m(35.0, Some(true))), 0.0);
        assert!((muscle_boost(&m(40.0, Some(true))) - 5.0).abs() < 1e-9);
        assert_eq!(muscle_boost(&m(38.0, Some(false))), 10.0);
        assert!((muscle_boost(&m(33.0, Some(false))) - 5.0).abs() < 1e-9);
        assert_eq!(muscle_boost(&m(20.0, Some(false))), 0.0);
    }

    #[test]
    fn muscle_boost_is_added_to_strength_only() {
        let base = compute_stats(&inputs(vec![lift(1, 10)], vec![]), now());
        let mut i = inputs(vec![lift(1, 10)], vec![]);
        i.muscle = Some(MuscleInput {
            skeletal_muscle_kg: 46.0,
            weight_kg: 100.0,
            male: Some(true),
        });
        let boosted = compute_stats(&i, now());
        assert_eq!(boosted.strength.value, base.strength.value + 10);
        assert_eq!(boosted.cardio, base.cardio);
        assert_eq!(boosted.focus, base.focus);
    }

    #[test]
    fn level_boundaries() {
        for (v, l) in [
            (0, 1),
            (24, 1),
            (25, 2),
            (49, 2),
            (50, 3),
            (74, 3),
            (75, 4),
            (100, 4),
        ] {
            assert_eq!(level_of(v), l, "value {v}");
        }
        assert_eq!(make_stat(25.0).progress, 0.0);
        assert_eq!(make_stat(100.0).progress, 1.0);
        assert_eq!(make_stat(75.0).progress, 0.0);
        assert!((make_stat(40.0).progress - 0.6).abs() < 1e-9);
    }

    #[test]
    fn strength_rises_monotonically_with_activity() {
        let mut prev = 0;
        for n in [1, 3, 6, 12] {
            let w = (0..n).map(|i| lift(i % 5, 10)).collect();
            let v = compute_stats(&inputs(w, vec![]), now()).strength.value;
            assert!(v > prev, "{n} sessions gave {v} <= {prev}");
            prev = v;
        }
        let a = compute_stats(&inputs(vec![lift(0, 5)], vec![]), now())
            .strength
            .value;
        let b = compute_stats(&inputs(vec![lift(0, 20)], vec![]), now())
            .strength
            .value;
        assert!(b > a);
    }

    #[test]
    fn diminishing_returns() {
        let at = |n: i64| {
            let w = (0..n).map(|_| lift(0, 10)).collect();
            f64::from(compute_stats(&inputs(w, vec![]), now()).strength.value)
        };
        assert!(at(2) - at(1) > at(12) - at(11));
    }

    #[test]
    fn cardio_rises_with_minutes_and_ignores_strength() {
        let few = compute_stats(&inputs(vec![run(0, 30.0)], vec![]), now())
            .cardio
            .value;
        let many = compute_stats(
            &inputs(vec![run(0, 30.0), run(1, 60.0), run(2, 60.0)], vec![]),
            now(),
        )
        .cardio
        .value;
        assert!(many > few);
        let lifting = compute_stats(&inputs(vec![lift(0, 20)], vec![]), now());
        assert_eq!(lifting.cardio.value, 10);
        let running = compute_stats(&inputs(vec![run(0, 60.0)], vec![]), now());
        assert_eq!(running.strength.value, 10);
    }

    #[test]
    fn focus_rises_with_sleep_and_regularity() {
        let base = compute_stats(&inputs(vec![lift(0, 5)], vec![]), now())
            .focus
            .value;
        let rested = (0..10).map(|d| night(d, 8.0)).collect();
        let slept = compute_stats(&inputs(vec![lift(0, 5)], rested), now())
            .focus
            .value;
        assert!(slept > base);
        let regular = (0..6).map(|d| lift(d, 5)).collect();
        let reg = compute_stats(&inputs(regular, vec![]), now()).focus.value;
        assert!(reg > base);
        // two workouts on the same day count as one active day
        let same = compute_stats(&inputs(vec![lift(0, 5), lift(0, 5)], vec![]), now())
            .focus
            .value;
        assert_eq!(same, base);
    }

    #[test]
    fn poor_sleep_lowers_every_stat_up_to_20_percent() {
        let w = || vec![lift(0, 20), lift(1, 20), run(0, 120.0), run(1, 60.0)];
        let with =
            |h: f64| compute_stats(&inputs(w(), (0..7).map(|d| night(d, h)).collect()), now());
        let (rested, bad, worst, awful) = (with(8.0), with(5.0), with(4.0), with(1.0));
        for (r, b, x, a) in [
            (
                rested.strength,
                bad.strength,
                worst.strength,
                awful.strength,
            ),
            (rested.cardio, bad.cardio, worst.cardio, awful.cardio),
        ] {
            assert!(b.value < r.value);
            assert!(x.value < b.value);
            assert_eq!(x.value, a.value, "penalty capped at 4h");
            let ratio = f64::from(x.value) / f64::from(r.value);
            assert!((0.77..=0.82).contains(&ratio), "ratio {ratio}");
        }
        // Focus also loses its own good-night component, so only check direction.
        assert!(worst.focus.value < bad.focus.value && bad.focus.value < rested.focus.value);
        assert_eq!(worst.focus.value, awful.focus.value);
    }

    #[test]
    fn no_penalty_at_six_hours_or_without_sleep_data() {
        let w = || vec![lift(0, 20)];
        let none = compute_stats(&inputs(w(), vec![]), now());
        let six = compute_stats(&inputs(w(), (0..7).map(|d| night(d, 6.0)).collect()), now());
        assert_eq!(none.strength, six.strength);
    }

    #[test]
    fn inactivity_decays_to_baseline() {
        let w = |ago: i64| (0..8).map(|i| lift(ago + i % 3, 10)).collect::<Vec<_>>();
        let fresh = compute_stats(&inputs(w(0), vec![]), now()).strength.value;
        let week_old = compute_stats(&inputs(w(7), vec![]), now()).strength.value;
        let stale = compute_stats(&inputs(w(13), vec![]), now()).strength.value;
        let dead = compute_stats(&inputs(w(14), vec![]), now()).strength.value;
        assert!(fresh > week_old && week_old > stale);
        assert_eq!(dead, 10);
        // sleep alone with no workouts never rises above baseline
        let only_sleep = (0..14).map(|d| night(d, 8.0)).collect();
        assert_eq!(
            compute_stats(&inputs(vec![], only_sleep), now())
                .focus
                .value,
            10
        );
    }

    #[test]
    fn values_are_clamped() {
        let w = (0..28)
            .flat_map(|d| [lift(d, 100), run(d, 600.0)])
            .collect();
        let s = compute_stats(&inputs(w, (0..14).map(|d| night(d, 8.0)).collect()), now());
        for st in [s.strength, s.cardio, s.focus] {
            assert!(st.value <= 100);
            assert!((0.0..=1.0).contains(&st.progress));
        }
        assert_eq!(s.strength.value, 100);
        assert_eq!(s.strength.level, 4);
        let empty = compute_stats(&Inputs::default(), now());
        assert_eq!(empty.strength.value, 10);
        assert_eq!(empty.strength.level, 1);
    }

    #[test]
    fn old_and_future_data_is_ignored() {
        let old = compute_stats(&inputs(vec![lift(40, 50)], vec![]), now());
        assert_eq!(old.strength.value, 10);
        let future = compute_stats(&inputs(vec![lift(-2, 50)], vec![]), now());
        assert_eq!(future.strength.value, 10);
    }
}

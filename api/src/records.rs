//! Personal bests per exercise: heaviest weight (any reps) and best estimated
//! 1RM (Epley: w * (1 + reps / 30)). See docs/progression.md.
//!
//! Detection is pure over [`SetRow`]s. A workout sets a personal best when its
//! best beats everything the user did in earlier workouts for that exercise;
//! the first time an exercise is logged has nothing to beat and is not a PR.
//! Sets with no weight (bodyweight, cardio, focus) never count.

use crate::{auth::AuthUser, error::AppError, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/records", get(get_records))
}

const EPS: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub struct SetRow {
    pub exercise_id: Uuid,
    pub exercise_name: String,
    pub workout_id: Uuid,
    pub at: DateTime<Utc>,
    pub reps: i32,
    pub weight_kg: f64,
}

/// Epley estimated one-rep max.
pub fn epley(weight_kg: f64, reps: i32) -> f64 {
    weight_kg * (1.0 + f64::from(reps) / 30.0)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExerciseRecord {
    pub exercise_id: Uuid,
    pub exercise_name: String,
    pub best_weight_kg: f64,
    pub best_weight_reps: i32,
    pub best_weight_at: DateTime<Utc>,
    pub best_e1rm_kg: f64,
    pub best_e1rm_weight_kg: f64,
    pub best_e1rm_reps: i32,
    pub best_e1rm_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PbKind {
    Weight,
    E1rm,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PersonalBest {
    pub exercise_id: Uuid,
    pub exercise_name: String,
    pub kind: PbKind,
    /// kg for `weight`, estimated 1RM in kg for `e1rm`.
    pub value: f64,
    pub previous: f64,
    pub weight_kg: f64,
    pub reps: i32,
    pub workout_id: Uuid,
    pub achieved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
struct Best {
    value: f64,
    weight_kg: f64,
    reps: i32,
    at: DateTime<Utc>,
    workout_id: Uuid,
}

/// (heaviest weight, best e1rm) per exercise; the earliest set to reach a value keeps it.
fn best_per_exercise<'a>(
    sets: impl Iterator<Item = &'a SetRow>,
) -> BTreeMap<Uuid, (String, Option<Best>, Option<Best>)> {
    let mut rows: Vec<&SetRow> = sets.filter(|s| s.weight_kg > 0.0 && s.reps > 0).collect();
    rows.sort_by_key(|s| s.at);
    let mut map: BTreeMap<Uuid, (String, Option<Best>, Option<Best>)> = BTreeMap::new();
    for s in rows {
        let entry = map
            .entry(s.exercise_id)
            .or_insert_with(|| (s.exercise_name.clone(), None, None));
        let mk = |value: f64| Best {
            value,
            weight_kg: s.weight_kg,
            reps: s.reps,
            at: s.at,
            workout_id: s.workout_id,
        };
        if entry.1.is_none_or(|b| s.weight_kg > b.value + EPS) {
            entry.1 = Some(mk(s.weight_kg));
        }
        let e = epley(s.weight_kg, s.reps);
        if entry.2.is_none_or(|b| e > b.value + EPS) {
            entry.2 = Some(mk(e));
        }
    }
    map
}

/// All-time bests per exercise, sorted by exercise name.
pub fn bests(sets: &[SetRow]) -> Vec<ExerciseRecord> {
    let mut out: Vec<ExerciseRecord> = best_per_exercise(sets.iter())
        .into_iter()
        .filter_map(|(id, (name, w, e))| {
            let (w, e) = (w?, e?);
            Some(ExerciseRecord {
                exercise_id: id,
                exercise_name: name,
                best_weight_kg: round2(w.value),
                best_weight_reps: w.reps,
                best_weight_at: w.at,
                best_e1rm_kg: round2(e.value),
                best_e1rm_weight_kg: e.weight_kg,
                best_e1rm_reps: e.reps,
                best_e1rm_at: e.at,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.exercise_name
            .to_lowercase()
            .cmp(&b.exercise_name.to_lowercase())
            .then(a.exercise_id.cmp(&b.exercise_id))
    });
    out
}

/// New personal bests set by `current` (one workout's sets) against `history`
/// (the user's earlier workouts). Output is sorted by exercise name, weight before e1rm.
pub fn detect(history: &[SetRow], current: &[SetRow]) -> Vec<PersonalBest> {
    let before = best_per_exercise(history.iter());
    let now = best_per_exercise(current.iter());
    let mut out = Vec::new();
    for (id, (name, w, e)) in now {
        let Some((_, pw, pe)) = before.get(&id) else {
            continue; // first time for this exercise: nothing to beat
        };
        for (kind, cur, prev) in [(PbKind::Weight, w, pw), (PbKind::E1rm, e, pe)] {
            let (Some(cur), Some(prev)) = (cur, prev) else {
                continue;
            };
            if cur.value > prev.value + EPS {
                out.push(PersonalBest {
                    exercise_id: id,
                    exercise_name: name.clone(),
                    kind,
                    value: round2(cur.value),
                    previous: round2(prev.value),
                    weight_kg: cur.weight_kg,
                    reps: cur.reps,
                    workout_id: cur.workout_id,
                    achieved_at: cur.at,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.exercise_name
            .to_lowercase()
            .cmp(&b.exercise_name.to_lowercase())
            .then((a.kind as u8).cmp(&(b.kind as u8)))
    });
    out
}

/// Personal bests set by one workout, judged against workouts that started before it.
pub fn in_workout(all: &[SetRow], workout_id: Uuid) -> Vec<PersonalBest> {
    let current: Vec<SetRow> = all
        .iter()
        .filter(|s| s.workout_id == workout_id)
        .cloned()
        .collect();
    let Some(at) = current.iter().map(|s| s.at).min() else {
        return Vec::new();
    };
    let history: Vec<SetRow> = all
        .iter()
        .filter(|s| s.workout_id != workout_id && s.at < at)
        .cloned()
        .collect();
    detect(&history, &current)
}

/// Personal bests of workouts that started in the last 24 hours, newest first.
pub fn recent(all: &[SetRow], now: DateTime<Utc>) -> Vec<PersonalBest> {
    let cutoff = now - Duration::hours(24);
    let mut ids: Vec<(DateTime<Utc>, Uuid)> = all
        .iter()
        .filter(|s| s.at > cutoff && s.at <= now)
        .map(|s| (s.at, s.workout_id))
        .collect();
    ids.sort();
    ids.dedup_by_key(|(_, id)| *id);
    let mut out: Vec<PersonalBest> = ids
        .into_iter()
        .flat_map(|(_, id)| in_workout(all, id))
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.achieved_at));
    out
}

type Row = (Uuid, String, Uuid, DateTime<Utc>, i32, f64);

/// Weighted sets of the user's live workouts, optionally up to `upto`.
pub async fn load_sets<'e>(
    exec: impl sqlx::PgExecutor<'e>,
    user_id: Uuid,
    upto: Option<DateTime<Utc>>,
) -> Result<Vec<SetRow>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.exercise_id, e.name, w.id, w.started_at, s.reps, s.weight_kg \
         FROM workout_sets s \
         JOIN workouts w ON w.id = s.workout_id \
         JOIN exercises e ON e.id = s.exercise_id \
         WHERE w.user_id = $1 AND w.deleted_at IS NULL AND s.weight_kg > 0 \
           AND ($2::timestamptz IS NULL OR w.started_at <= $2)",
    )
    .bind(user_id)
    .bind(upto)
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(exercise_id, exercise_name, workout_id, at, reps, weight_kg)| SetRow {
                exercise_id,
                exercise_name,
                workout_id,
                at,
                reps,
                weight_kg,
            },
        )
        .collect())
}

pub async fn recent_personal_bests(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<PersonalBest>, sqlx::Error> {
    let all = load_sets(pool, user_id, Some(now)).await?;
    Ok(recent(&all, now))
}

/// Personal bests set by the given workout (used by the workout endpoints).
pub async fn workout_personal_bests<'e>(
    exec: impl sqlx::PgExecutor<'e>,
    user_id: Uuid,
    workout_id: Uuid,
) -> Result<Vec<PersonalBest>, sqlx::Error> {
    let all = load_sets(exec, user_id, None).await?;
    Ok(in_workout(&all, workout_id))
}

async fn get_records(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ExerciseRecord>>, AppError> {
    if !state.config.feature("stats") {
        return Err(AppError::NotFound);
    }
    let all = load_sets(&state.pool, user.0, None).await?;
    Ok(Json(bests(&all)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, d, 10, 0, 0).unwrap()
    }
    fn ex(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    fn set(e: u128, name: &str, w: u128, d: u32, reps: i32, kg: f64) -> SetRow {
        SetRow {
            exercise_id: ex(e),
            exercise_name: name.into(),
            workout_id: Uuid::from_u128(w),
            at: day(d),
            reps,
            weight_kg: kg,
        }
    }

    #[test]
    fn epley_formula() {
        assert!((epley(100.0, 5) - 116.666_666_7).abs() < 1e-6);
        assert!((epley(60.0, 30) - 120.0).abs() < 1e-9);
        assert!((epley(100.0, 1) - 103.333_333_3).abs() < 1e-6);
    }

    #[test]
    fn bests_track_weight_and_e1rm_separately() {
        let sets = [
            set(1, "Squat", 10, 1, 5, 100.0), // e1rm 116.67
            set(1, "Squat", 10, 1, 1, 110.0), // heaviest, e1rm 113.67
            set(1, "Squat", 11, 2, 12, 90.0), // e1rm 126
            set(2, "Bench Press", 10, 1, 5, 60.0),
        ];
        let b = bests(&sets);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].exercise_name, "Bench Press");
        let sq = &b[1];
        assert_eq!((sq.best_weight_kg, sq.best_weight_reps), (110.0, 1));
        assert_eq!(sq.best_weight_at, day(1));
        assert_eq!(sq.best_e1rm_kg, 126.0);
        assert_eq!((sq.best_e1rm_weight_kg, sq.best_e1rm_reps), (90.0, 12));
        assert_eq!(sq.best_e1rm_at, day(2));
    }

    #[test]
    fn unweighted_sets_never_count() {
        let sets = [set(1, "Pull-Up", 10, 1, 8, 0.0)];
        assert!(bests(&sets).is_empty());
        assert!(detect(&sets, &sets).is_empty());
    }

    #[test]
    fn ties_keep_the_earliest() {
        let sets = [
            set(1, "Squat", 11, 5, 5, 100.0),
            set(1, "Squat", 10, 2, 5, 100.0),
        ];
        let b = bests(&sets);
        assert_eq!(b[0].best_weight_at, day(2));
    }

    #[test]
    fn detects_weight_and_e1rm_prs() {
        let history = [set(1, "Squat", 10, 1, 5, 100.0)];
        // heavier single: weight PR; e1rm 106 < 116.67 so no e1rm PR
        let cur = [set(1, "Squat", 11, 3, 1, 105.0)];
        let prs = detect(&history, &cur);
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].kind, PbKind::Weight);
        assert_eq!((prs[0].value, prs[0].previous), (105.0, 100.0));
        assert_eq!(prs[0].workout_id, Uuid::from_u128(11));
        // more reps at the same weight: e1rm PR only
        let cur = [set(1, "Squat", 11, 3, 8, 100.0)];
        let prs = detect(&history, &cur);
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].kind, PbKind::E1rm);
        assert_eq!(prs[0].value, 126.67);
        assert_eq!(prs[0].previous, 116.67);
        // heavier and more volume: both
        let cur = [set(1, "Squat", 11, 3, 5, 110.0)];
        let kinds: Vec<_> = detect(&history, &cur).iter().map(|p| p.kind).collect();
        assert_eq!(kinds, [PbKind::Weight, PbKind::E1rm]);
    }

    #[test]
    fn equal_or_lower_is_not_a_pr_and_first_time_is_not_a_pr() {
        let history = [set(1, "Squat", 10, 1, 5, 100.0)];
        assert!(detect(&history, &[set(1, "Squat", 11, 3, 5, 100.0)]).is_empty());
        assert!(detect(&history, &[set(1, "Squat", 11, 3, 3, 95.0)]).is_empty());
        // a different exercise with no history
        assert!(detect(&history, &[set(2, "Bench Press", 11, 3, 5, 200.0)]).is_empty());
    }

    #[test]
    fn in_workout_only_compares_with_earlier_workouts() {
        let all = [
            set(1, "Squat", 10, 1, 5, 100.0),
            set(1, "Squat", 11, 3, 5, 110.0),
            set(1, "Squat", 12, 5, 5, 120.0),
        ];
        // the middle workout beat the first, but not the later one (which does not matter)
        let mid = in_workout(&all, Uuid::from_u128(11));
        assert_eq!(mid[0].kind, PbKind::Weight);
        assert_eq!(mid[0].previous, 100.0);
        let first = in_workout(&all, Uuid::from_u128(10));
        assert!(first.is_empty());
        let last = in_workout(&all, Uuid::from_u128(12));
        assert_eq!(last[0].previous, 110.0);
        assert!(in_workout(&all, Uuid::from_u128(99)).is_empty());
    }

    #[test]
    fn recent_lists_prs_from_last_24_hours() {
        let all = [
            set(1, "Squat", 10, 1, 5, 100.0),
            set(1, "Squat", 11, 3, 5, 110.0),
            set(1, "Squat", 12, 5, 5, 120.0),
        ];
        let now = day(5) + Duration::hours(6);
        let r = recent(&all, now);
        assert!(r.iter().all(|p| p.workout_id == Uuid::from_u128(12)));
        assert_eq!(r.len(), 2);
        // 25h later nothing is recent
        assert!(recent(&all, day(5) + Duration::hours(25)).is_empty());
        // future workouts are ignored
        assert!(recent(&all, day(4)).iter().all(|p| p.achieved_at <= day(4)));
    }
}

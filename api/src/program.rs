//! Built-in starter program "Strength basics" and its weight progression.
//! See docs/progression.md.
//!
//! 3 days a week for 4 weeks, alternating two sessions (A, B). Weeks 3 and 4
//! add a fourth set. Target weights come from the user's last sessions of each
//! exercise: +increment after a session where every reps target was met,
//! -10 % after two sessions in a row with missed reps. All decisions are pure
//! functions over [`SessionPerf`].

use crate::{auth::AuthUser, error::AppError, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/program", get(get_program))
}

pub const PROGRAM_NAME: &str = "Strength basics";
pub const DAYS_PER_WEEK: usize = 3;
pub const WEEKS: usize = 4;
pub const BASE_SETS: u32 = 3;
pub const DELOAD_FACTOR: f64 = 0.9;
const PLATE_STEP: f64 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lift {
    /// Catalog exercise name.
    pub name: &'static str,
    pub reps: i32,
    /// kg added after a fully completed session (0 = no automatic increase).
    pub increment: f64,
}

pub const SQUAT: Lift = Lift {
    name: "Squat",
    reps: 5,
    increment: 2.5,
};
pub const SWING: Lift = Lift {
    name: "Kettlebell Swing",
    reps: 15,
    increment: 0.0,
};
pub const BENCH: Lift = Lift {
    name: "Bench Press",
    reps: 5,
    increment: 2.5,
};
pub const DEADLIFT: Lift = Lift {
    name: "Deadlift",
    reps: 5,
    increment: 5.0,
};
pub const PULL_UP: Lift = Lift {
    name: "Pull-Up",
    reps: 5,
    increment: 0.0,
};

/// One planned exercise; `superset` pairs the swing with the squat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub lift: Lift,
    pub superset: Option<char>,
}

const fn slot(lift: Lift, superset: Option<char>) -> Slot {
    Slot { lift, superset }
}

pub const SESSION_A: [Slot; 4] = [
    slot(SQUAT, Some('a')),
    slot(SWING, Some('a')),
    slot(BENCH, None),
    slot(PULL_UP, None),
];
pub const SESSION_B: [Slot; 3] = [slot(DEADLIFT, None), slot(BENCH, None), slot(PULL_UP, None)];

pub const ALL_LIFTS: [Lift; 5] = [SQUAT, SWING, BENCH, DEADLIFT, PULL_UP];

/// Where `done` program sessions put the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Position {
    /// 1-based, capped at 4.
    pub week: usize,
    /// 1-based day within the week.
    pub day: usize,
    pub sets: u32,
    pub completed: bool,
    #[serde(skip)]
    pub is_a: bool,
}

pub fn position(done: usize) -> Position {
    let week = (done / DAYS_PER_WEEK).min(WEEKS - 1) + 1;
    Position {
        week,
        day: done % DAYS_PER_WEEK + 1,
        sets: if week >= 3 { BASE_SETS + 1 } else { BASE_SETS },
        completed: done >= DAYS_PER_WEEK * WEEKS,
        is_a: done.is_multiple_of(2),
    }
}

pub fn slots(is_a: bool) -> &'static [Slot] {
    if is_a {
        &SESSION_A
    } else {
        &SESSION_B
    }
}

/// What the user did for one exercise in one workout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionPerf {
    /// Working weight: the heaviest set.
    pub weight_kg: f64,
    /// Sets at the working weight.
    pub working_sets: usize,
    /// Fewest reps among the working sets.
    pub min_reps: i32,
}

/// Builds the perf of one workout from its (reps, weight) sets of an exercise.
pub fn perf_from_sets(sets: &[(i32, f64)]) -> Option<SessionPerf> {
    let weight = sets.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    if !weight.is_finite() {
        return None;
    }
    let working: Vec<i32> = sets
        .iter()
        .filter(|s| s.1 >= weight - 1e-9)
        .map(|s| s.0)
        .collect();
    Some(SessionPerf {
        weight_kg: weight,
        working_sets: working.len(),
        min_reps: working.iter().copied().min().unwrap_or(0),
    })
}

/// All reps done: at least `min_sets` working sets, each with `target_reps` or more.
pub fn completed(p: &SessionPerf, target_reps: i32, min_sets: u32) -> bool {
    p.working_sets >= min_sets as usize && p.min_reps >= target_reps
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// No history: the user picks a starting weight.
    Start,
    Increase,
    /// Same weight again (missed reps once, or no automatic increase).
    Repeat,
    /// Two sessions in a row with missed reps: -10 %.
    Deload,
}

pub fn round_to_step(v: f64) -> f64 {
    ((v / PLATE_STEP).round() * PLATE_STEP).max(0.0)
}

/// Next target weight from the last sessions (newest first).
pub fn next_weight(history: &[SessionPerf], lift: &Lift, min_sets: u32) -> (Option<f64>, Reason) {
    let Some(last) = history.first() else {
        return (None, Reason::Start);
    };
    let ok = |p: &SessionPerf| completed(p, lift.reps, min_sets);
    if history.len() >= 2 && !ok(last) && !ok(&history[1]) {
        let w = if last.weight_kg > 0.0 {
            round_to_step(last.weight_kg * DELOAD_FACTOR)
        } else {
            0.0
        };
        return (Some(w), Reason::Deload);
    }
    if ok(last) && lift.increment > 0.0 {
        return (Some(last.weight_kg + lift.increment), Reason::Increase);
    }
    (Some(last.weight_kg), Reason::Repeat)
}

// ---------- response ----------

#[derive(Debug, Serialize)]
pub struct ExerciseOut {
    pub exercise_id: Option<Uuid>,
    pub name: &'static str,
    pub sets: u32,
    pub reps: i32,
    pub weight_kg: Option<f64>,
    pub reason: Reason,
    pub note: &'static str,
    pub superset: Option<char>,
}

#[derive(Debug, Serialize)]
pub struct RestOut {
    pub min_s: u32,
    pub max_s: u32,
}

#[derive(Debug, Serialize)]
pub struct Program {
    pub name: &'static str,
    pub title: &'static str,
    #[serde(flatten)]
    pub position: Position,
    pub weeks: usize,
    pub days_per_week: usize,
    pub sessions_done: usize,
    pub rest: RestOut,
    pub exercises: Vec<ExerciseOut>,
}

fn note(reason: Reason, inc: f64) -> &'static str {
    match reason {
        Reason::Start => "Pick a starting weight",
        Reason::Increase => "All reps done last time, add weight",
        Reason::Repeat if inc > 0.0 => "Missed reps last time, repeat the weight",
        Reason::Repeat => "Same as last time",
        Reason::Deload => "Missed reps twice in a row, 10% lighter",
    }
}

/// Pure assembly. `history` maps an exercise name to its sessions, newest first.
pub fn build(
    sessions_done: usize,
    history: &BTreeMap<&str, Vec<SessionPerf>>,
    ids: &BTreeMap<String, Uuid>,
) -> Program {
    let pos = position(sessions_done);
    let exercises = slots(pos.is_a)
        .iter()
        .map(|s| {
            let h = history.get(s.lift.name).map_or(&[][..], Vec::as_slice);
            let (weight_kg, reason) = next_weight(h, &s.lift, BASE_SETS);
            ExerciseOut {
                exercise_id: ids.get(s.lift.name).copied(),
                name: s.lift.name,
                sets: pos.sets,
                reps: s.lift.reps,
                weight_kg,
                reason,
                note: note(reason, s.lift.increment),
                superset: s.superset,
            }
        })
        .collect();
    Program {
        name: PROGRAM_NAME,
        title: if pos.is_a { "Workout A" } else { "Workout B" },
        position: pos,
        weeks: WEEKS,
        days_per_week: DAYS_PER_WEEK,
        sessions_done,
        rest: RestOut {
            min_s: 60,
            max_s: 120,
        },
        exercises,
    }
}

// ---------- loading ----------

type Row = (Uuid, DateTime<Utc>, String, i32, f64);

pub async fn for_user(pool: &PgPool, user_id: Uuid) -> Result<Program, sqlx::Error> {
    let names: Vec<&str> = ALL_LIFTS.iter().map(|l| l.name).collect();
    let id_rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM exercises WHERE owner_id IS NULL AND name = ANY($1)")
            .bind(&names)
            .fetch_all(pool)
            .await?;
    let ids: BTreeMap<String, Uuid> = id_rows.into_iter().map(|(i, n)| (n, i)).collect();

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT w.id, w.started_at, e.name, s.reps, s.weight_kg \
         FROM workout_sets s \
         JOIN workouts w ON w.id = s.workout_id \
         JOIN exercises e ON e.id = s.exercise_id \
         WHERE w.user_id = $1 AND w.deleted_at IS NULL \
           AND e.owner_id IS NULL AND e.name = ANY($2) \
         ORDER BY w.started_at DESC, w.id",
    )
    .bind(user_id)
    .bind(&names)
    .fetch_all(pool)
    .await?;

    let done: BTreeSet<Uuid> = rows.iter().map(|r| r.0).collect();
    // exercise -> workout (newest first) -> sets
    let mut order: Vec<(Uuid, &str)> = Vec::new();
    let mut sets: BTreeMap<(Uuid, &str), Vec<(i32, f64)>> = BTreeMap::new();
    for (wid, _, name, reps, kg) in &rows {
        let key = (*wid, name.as_str());
        if !sets.contains_key(&key) {
            order.push(key);
        }
        sets.entry(key).or_default().push((*reps, *kg));
    }
    let mut history: BTreeMap<&str, Vec<SessionPerf>> = BTreeMap::new();
    for key in order {
        if let Some(p) = perf_from_sets(&sets[&key]) {
            history.entry(key.1).or_default().push(p);
        }
    }
    Ok(build(done.len(), &history, &ids))
}

async fn get_program(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Program>, AppError> {
    if !state.config.feature("stats") {
        return Err(AppError::NotFound);
    }
    Ok(Json(for_user(&state.pool, user.0).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perf(weight: f64, sets: usize, min_reps: i32) -> SessionPerf {
        SessionPerf {
            weight_kg: weight,
            working_sets: sets,
            min_reps,
        }
    }

    #[test]
    fn position_by_sessions_done() {
        let p = position(0);
        assert_eq!(
            (p.week, p.day, p.sets, p.completed, p.is_a),
            (1, 1, 3, false, true)
        );
        let p = position(2);
        assert_eq!((p.week, p.day, p.is_a), (1, 3, true));
        let p = position(3);
        assert_eq!((p.week, p.day, p.is_a), (2, 1, false));
        assert_eq!(position(5).sets, 3);
        let p = position(6);
        assert_eq!((p.week, p.day, p.sets), (3, 1, 4));
        assert_eq!(position(11).week, 4);
        assert_eq!(position(11).sets, 4);
        assert!(!position(11).completed);
        let p = position(12);
        assert!(p.completed);
        assert_eq!((p.week, p.sets), (4, 4));
    }

    #[test]
    fn perf_uses_the_heaviest_sets_as_working_sets() {
        let p = perf_from_sets(&[(5, 40.0), (5, 100.0), (5, 100.0), (4, 100.0)]).unwrap();
        assert_eq!(p.weight_kg, 100.0);
        assert_eq!(p.working_sets, 3);
        assert_eq!(p.min_reps, 4);
        assert!(perf_from_sets(&[]).is_none());
        let bw = perf_from_sets(&[(8, 0.0), (6, 0.0)]).unwrap();
        assert_eq!((bw.weight_kg, bw.working_sets, bw.min_reps), (0.0, 2, 6));
    }

    #[test]
    fn completed_needs_sets_and_reps() {
        assert!(completed(&perf(100.0, 3, 5), 5, 3));
        assert!(completed(&perf(100.0, 4, 6), 5, 3));
        assert!(!completed(&perf(100.0, 2, 5), 5, 3));
        assert!(!completed(&perf(100.0, 3, 4), 5, 3));
        // week 3 asks for four sets
        assert!(!completed(&perf(100.0, 3, 5), 5, 4));
    }

    #[test]
    fn no_history_means_pick_a_weight() {
        let (w, r) = next_weight(&[], &SQUAT, 3);
        assert_eq!((w, r), (None, Reason::Start));
    }

    #[test]
    fn weight_increases_by_exercise_after_a_good_session() {
        let good = [perf(100.0, 3, 5)];
        assert_eq!(
            next_weight(&good, &SQUAT, 3),
            (Some(102.5), Reason::Increase)
        );
        assert_eq!(
            next_weight(&good, &BENCH, 3),
            (Some(102.5), Reason::Increase)
        );
        assert_eq!(
            next_weight(&good, &DEADLIFT, 3),
            (Some(105.0), Reason::Increase)
        );
        // pull-ups and swings keep their weight
        assert_eq!(
            next_weight(&[perf(0.0, 3, 5)], &PULL_UP, 3),
            (Some(0.0), Reason::Repeat)
        );
        assert_eq!(
            next_weight(&[perf(24.0, 3, 15)], &SWING, 3),
            (Some(24.0), Reason::Repeat)
        );
        // swing reps target is 15
        assert_eq!(
            next_weight(&[perf(24.0, 3, 14)], &SWING, 3).1,
            Reason::Repeat
        );
    }

    #[test]
    fn one_miss_repeats_two_misses_deload() {
        let bad = perf(100.0, 3, 4);
        let good = perf(100.0, 3, 5);
        assert_eq!(
            next_weight(&[bad], &SQUAT, 3),
            (Some(100.0), Reason::Repeat)
        );
        // good then bad (newest first: bad, good) is only one miss in a row
        assert_eq!(
            next_weight(&[bad, good], &SQUAT, 3),
            (Some(100.0), Reason::Repeat)
        );
        // two in a row: -10 %, rounded to 2.5 kg
        assert_eq!(
            next_weight(&[bad, bad], &SQUAT, 3),
            (Some(90.0), Reason::Deload)
        );
        assert_eq!(
            next_weight(&[perf(62.5, 3, 3), perf(62.5, 3, 4)], &BENCH, 3),
            (Some(57.5), Reason::Deload)
        );
        // an older miss does not matter once there was a success in between
        assert_eq!(
            next_weight(&[good, bad, bad], &SQUAT, 3),
            (Some(102.5), Reason::Increase)
        );
        // deload of an unweighted pull-up stays at 0
        assert_eq!(
            next_weight(&[perf(0.0, 3, 3), perf(0.0, 3, 2)], &PULL_UP, 3),
            (Some(0.0), Reason::Deload)
        );
    }

    #[test]
    fn rounding_to_plates() {
        assert_eq!(round_to_step(90.0), 90.0);
        assert_eq!(round_to_step(56.25), 57.5);
        assert_eq!(round_to_step(91.3), 92.5);
        assert_eq!(round_to_step(-3.0), 0.0);
    }

    #[test]
    fn build_fills_session_a_and_b() {
        let mut hist: BTreeMap<&str, Vec<SessionPerf>> = BTreeMap::new();
        hist.insert("Squat", vec![perf(100.0, 3, 5)]);
        hist.insert("Deadlift", vec![perf(140.0, 3, 4), perf(140.0, 3, 3)]);
        let mut ids = BTreeMap::new();
        ids.insert("Squat".to_string(), Uuid::from_u128(1));

        let a = build(0, &hist, &ids);
        assert_eq!(a.title, "Workout A");
        let names: Vec<_> = a.exercises.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            ["Squat", "Kettlebell Swing", "Bench Press", "Pull-Up"]
        );
        assert_eq!(a.exercises[0].weight_kg, Some(102.5));
        assert_eq!(a.exercises[0].exercise_id, Some(Uuid::from_u128(1)));
        assert_eq!(a.exercises[1].exercise_id, None);
        assert_eq!(a.exercises[0].superset, Some('a'));
        assert_eq!(a.exercises[1].superset, Some('a'));
        assert_eq!(a.exercises[2].superset, None);
        assert_eq!(a.exercises[2].weight_kg, None);
        assert_eq!(a.exercises[2].note, "Pick a starting weight");
        assert_eq!((a.exercises[1].sets, a.exercises[1].reps), (3, 15));
        assert_eq!(a.sessions_done, 0);

        let b = build(7, &hist, &ids);
        assert_eq!(b.title, "Workout B");
        assert_eq!((b.position.week, b.position.day), (3, 2));
        assert_eq!(b.exercises[0].name, "Deadlift");
        assert_eq!(b.exercises[0].weight_kg, Some(125.0)); // 140 * 0.9 = 126 -> 125
        assert_eq!(b.exercises[0].reason, Reason::Deload);
        assert!(b.exercises.iter().all(|e| e.sets == 4));

        let json = serde_json::to_value(&b).unwrap();
        assert_eq!(json["name"], "Strength basics");
        assert_eq!(json["week"], 3);
        assert_eq!(json["rest"]["min_s"], 60);
        assert_eq!(json["exercises"][0]["reason"], "deload");
    }
}

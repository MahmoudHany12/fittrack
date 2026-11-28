//! Progression: workout windows, levels, recovery, daily ring and hints.
//! See docs/progression.md.
//!
//! Everything that decides anything is a pure function over plain inputs
//! ([`Inputs`]: sessions, protein entries, sleep intervals) plus `now`.
//! [`load_inputs`] fetches them (every query filters by user id) and
//! [`for_user`] ties the two together.

use crate::{auth::AuthUser, error::AppError, records, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/progression", get(get_progression))
}

/// How far back history is loaded. Points are derived from this much history.
pub const LOOKBACK_DAYS: i64 = 365;
/// Level thresholds in points: L1, L2, L3, L4.
pub const THRESHOLDS: [i64; 4] = [0, 4, 12, 24];
pub const WINDOW_HOURS: i64 = 48;
pub const REST_SLEEP_MINUTES: f64 = 6.0 * 60.0;
pub const PROTEIN_BONUS_G: f64 = 25.0;
pub const PROTEIN_BONUS_HOURS: i64 = 2;
pub const RING_GOAL_MINUTES: i64 = 30;
pub const FUEL_HOURS: i64 = 4;

pub const MSG_RING_DONE: &str = "Done for today \u{2014} extra training won't add more.";
pub const MSG_WORKOUT_DONE: &str = "Workout complete \u{2014} time to rest and recover.";
pub const MSG_PROTEIN: &str = "Have ~25 g protein in the next 2 hours to recover twice as fast.";
pub const MSG_FUEL: &str = "Eat something before training for more energy.";

// ---------- inputs ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Strength,
    Cardio,
    Focus,
}

impl Track {
    /// Recovery before the window opens.
    pub fn recovery(self) -> Duration {
        match self {
            Track::Strength => Duration::hours(20),
            Track::Cardio => Duration::hours(12),
            Track::Focus => Duration::zero(),
        }
    }
}

/// One counted session for a track (one workout can yield several).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub at: DateTime<Utc>,
    pub track: Track,
}

/// A food log entry: when it was logged and its protein.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProteinEntry {
    pub at: DateTime<Utc>,
    pub grams: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SleepInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl SleepInterval {
    pub fn minutes(&self) -> f64 {
        (self.end - self.start).num_seconds() as f64 / 60.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub sessions: Vec<Session>,
    pub protein: Vec<ProteinEntry>,
    pub sleeps: Vec<SleepInterval>,
    /// Workout minutes today plus steps / 100.
    pub active_minutes_today: i64,
}

// ---------- levels ----------

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LevelInfo {
    pub level: u8,
    pub points: i64,
    pub progress: f64,
    pub value: u8,
}

/// Level 1-4, progress toward the next level and the 0-100 ring value.
pub fn level_info(points: i64) -> LevelInfo {
    let points = points.max(0);
    let idx = THRESHOLDS
        .iter()
        .rposition(|t| points >= *t)
        .unwrap_or_default();
    let level = idx as u8 + 1;
    let progress = if idx == THRESHOLDS.len() - 1 {
        1.0
    } else {
        let lo = THRESHOLDS[idx];
        let hi = THRESHOLDS[idx + 1];
        ((points - lo) as f64 / (hi - lo) as f64).clamp(0.0, 1.0)
    };
    let value = (f64::from(level - 1) * 25.0 + 25.0 * progress).round() as u8;
    LevelInfo {
        level,
        points,
        progress,
        value,
    }
}

// ---------- window state machine ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    Open,
    Recovering,
    NeedsRest,
    Missed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WindowTrack {
    pub state: WindowState,
    pub opens_at: Option<DateTime<Utc>>,
    pub closes_at: Option<DateTime<Utc>>,
    pub fraction: f64,
    pub protein_bonus: bool,
    #[serde(flatten)]
    pub level: LevelInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FocusTrack {
    #[serde(flatten)]
    pub level: LevelInfo,
}

#[derive(Debug, Clone, Copy)]
struct Cycle {
    anchor: DateTime<Utc>,
    bonus: bool,
    recovery_end: DateTime<Utc>,
    /// When the window opens; `None` while a night of sleep is still required.
    open_at: Option<DateTime<Utc>>,
}

/// True when protein of at least 25 g was logged within 2 h after `t`.
pub fn protein_bonus(t: DateTime<Utc>, protein: &[ProteinEntry]) -> bool {
    let end = t + Duration::hours(PROTEIN_BONUS_HOURS);
    protein_between(protein, t, end) >= PROTEIN_BONUS_G
}

fn protein_between(protein: &[ProteinEntry], from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
    protein
        .iter()
        .filter(|p| p.at >= from && p.at <= to)
        .map(|p| p.grams.max(0.0))
        .sum()
}

/// The rest rule applies only when any sleep was logged in the last 7 days.
pub fn rest_rule_active(sleeps: &[SleepInterval], now: DateTime<Utc>) -> bool {
    sleeps
        .iter()
        .any(|s| s.end <= now && s.end > now - Duration::days(7))
}

fn new_cycle(
    kind: Track,
    anchor: DateTime<Utc>,
    protein: &[ProteinEntry],
    sleeps: &[SleepInterval],
    now: DateTime<Utc>,
    rest_active: bool,
) -> Cycle {
    let bonus = protein_bonus(anchor, protein);
    let full = kind.recovery();
    let recovery = if bonus { full / 2 } else { full };
    let recovery_end = anchor + recovery;
    let open_at = if rest_active {
        sleeps
            .iter()
            .filter(|s| s.end > anchor && s.end <= now && s.minutes() >= REST_SLEEP_MINUTES)
            .map(|s| s.end)
            .min()
            .map(|e| e.max(recovery_end))
    } else {
        Some(recovery_end)
    };
    Cycle {
        anchor,
        bonus,
        recovery_end,
        open_at,
    }
}

/// Simulates a strength or cardio track from history. Sessions need not be sorted.
pub fn window_track(
    kind: Track,
    sessions: &[DateTime<Utc>],
    protein: &[ProteinEntry],
    sleeps: &[SleepInterval],
    now: DateTime<Utc>,
) -> WindowTrack {
    let window = Duration::hours(WINDOW_HOURS);
    let rest_active = rest_rule_active(sleeps, now);
    let mut times: Vec<DateTime<Utc>> = sessions.iter().copied().filter(|t| *t <= now).collect();
    times.sort();

    let mut points: i64 = 0;
    let mut cycle: Option<Cycle> = None;
    for s in times {
        let Some(c) = cycle else {
            points += 1;
            cycle = Some(new_cycle(kind, s, protein, sleeps, now, rest_active));
            continue;
        };
        let Some(open) = c.open_at else {
            continue; // still needs rest: logged, not counted
        };
        if s < open {
            continue; // still recovering: logged, not counted
        }
        let close = open + window;
        if s <= close {
            points += 1;
        } else {
            points = (points - missed_windows(close, s)).max(0);
        }
        cycle = Some(new_cycle(kind, s, protein, sleeps, now, rest_active));
    }

    let Some(c) = cycle else {
        return WindowTrack {
            state: WindowState::Open,
            opens_at: None,
            closes_at: None,
            fraction: 1.0,
            protein_bonus: false,
            level: level_info(points),
        };
    };
    let progress = |to: DateTime<Utc>| {
        let total = (to - c.anchor).num_seconds().max(1) as f64;
        (((now - c.anchor).num_seconds()) as f64 / total).clamp(0.0, 1.0)
    };
    let (state, opens_at, closes_at, fraction) = match c.open_at {
        None if now < c.recovery_end => (
            WindowState::Recovering,
            Some(c.recovery_end),
            None,
            progress(c.recovery_end),
        ),
        None => (WindowState::NeedsRest, None, None, 1.0),
        Some(open) if now < open => (
            WindowState::Recovering,
            Some(open),
            Some(open + window),
            progress(open),
        ),
        Some(open) => {
            let close = open + window;
            if now <= close {
                let left = (close - now).num_seconds() as f64 / window.num_seconds() as f64;
                (
                    WindowState::Open,
                    Some(open),
                    Some(close),
                    left.clamp(0.0, 1.0),
                )
            } else {
                points = (points - missed_windows(close, now)).max(0);
                (WindowState::Missed, Some(open), Some(close), 0.0)
            }
        }
    };
    WindowTrack {
        state,
        opens_at,
        closes_at,
        fraction,
        protein_bonus: c.bonus,
        level: level_info(points),
    }
}

/// Windows that closed unused: the first one at `close`, then one more for every
/// further 48 h without a session, up to `until`.
pub fn missed_windows(close: DateTime<Utc>, until: DateTime<Utc>) -> i64 {
    if until <= close {
        return 0;
    }
    1 + (until - close).num_seconds() / (WINDOW_HOURS * 3600)
}

/// Focus only gains: +1 per distinct day with a focus session.
pub fn focus_track(sessions: &[DateTime<Utc>], now: DateTime<Utc>) -> FocusTrack {
    let days: BTreeSet<NaiveDate> = sessions
        .iter()
        .filter(|t| **t <= now)
        .map(|t| t.date_naive())
        .collect();
    FocusTrack {
        level: level_info(days.len() as i64),
    }
}

// ---------- ring and hints ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Ring {
    pub minutes: i64,
    pub goal: i64,
    pub closed: bool,
}

pub fn ring(active_minutes_today: i64) -> Ring {
    let minutes = active_minutes_today.max(0);
    Ring {
        minutes,
        goal: RING_GOAL_MINUTES,
        closed: minutes >= RING_GOAL_MINUTES,
    }
}

pub fn hints(
    ring: &Ring,
    strength: &WindowTrack,
    cardio: &WindowTrack,
    sessions: &[Session],
    protein: &[ProteinEntry],
    now: DateTime<Utc>,
) -> Vec<String> {
    let mut out = Vec::new();
    if ring.closed {
        out.push(MSG_RING_DONE.to_string());
    }
    let last = sessions
        .iter()
        .filter(|s| s.track != Track::Focus && s.at <= now)
        .map(|s| s.at)
        .max();
    if let Some(t) = last {
        let post = Duration::hours(PROTEIN_BONUS_HOURS);
        if now - t <= post {
            out.push(MSG_WORKOUT_DONE.to_string());
            if protein_between(protein, t, t + post) < PROTEIN_BONUS_G {
                out.push(MSG_PROTEIN.to_string());
            }
        }
    }
    let window_open = strength.state == WindowState::Open || cardio.state == WindowState::Open;
    let fed = protein
        .iter()
        .any(|p| p.at <= now && now - p.at < Duration::hours(FUEL_HOURS));
    if window_open && !fed {
        out.push(MSG_FUEL.to_string());
    }
    out
}

// ---------- response ----------

#[derive(Debug, Clone, Serialize)]
pub struct Tracks {
    pub strength: WindowTrack,
    pub cardio: WindowTrack,
    pub focus: FocusTrack,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progression {
    pub tracks: Tracks,
    pub ring: Ring,
    pub hints: Vec<String>,
    pub new_personal_bests: Vec<records::PersonalBest>,
}

/// Pure: everything except personal bests.
pub fn compute(inputs: &Inputs, now: DateTime<Utc>) -> Progression {
    let at = |t: Track| -> Vec<DateTime<Utc>> {
        inputs
            .sessions
            .iter()
            .filter(|s| s.track == t)
            .map(|s| s.at)
            .collect()
    };
    let strength = window_track(
        Track::Strength,
        &at(Track::Strength),
        &inputs.protein,
        &inputs.sleeps,
        now,
    );
    let cardio = window_track(
        Track::Cardio,
        &at(Track::Cardio),
        &inputs.protein,
        &inputs.sleeps,
        now,
    );
    let focus = focus_track(&at(Track::Focus), now);
    let ring = ring(inputs.active_minutes_today);
    let hints = hints(
        &ring,
        &strength,
        &cardio,
        &inputs.sessions,
        &inputs.protein,
        now,
    );
    Progression {
        tracks: Tracks {
            strength,
            cardio,
            focus,
        },
        ring,
        hints,
        new_personal_bests: Vec::new(),
    }
}

// ---------- loading ----------

fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.from_utc_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default())
}

/// Maps a workout's set counts to the tracks it counts for.
pub fn tracks_for(strength: i64, cardio: i64, focus: i64) -> Vec<Track> {
    let mut out = Vec::new();
    if strength > 0 {
        out.push(Track::Strength);
    }
    if cardio > 0 || strength + cardio + focus == 0 {
        out.push(Track::Cardio);
    }
    if focus > 0 {
        out.push(Track::Focus);
    }
    out
}

pub async fn load_inputs(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Inputs, sqlx::Error> {
    let since = now - Duration::days(LOOKBACK_DAYS);
    let rows: Vec<(DateTime<Utc>, i64, i64, i64)> = sqlx::query_as(
        "SELECT w.started_at, \
                COUNT(s.id) FILTER (WHERE e.muscle_group NOT IN ('cardio', 'focus')), \
                COUNT(s.id) FILTER (WHERE e.muscle_group = 'cardio'), \
                COUNT(s.id) FILTER (WHERE e.muscle_group = 'focus') \
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
    let sessions = rows
        .into_iter()
        .flat_map(|(at, st, ca, fo)| {
            tracks_for(st, ca, fo)
                .into_iter()
                .map(move |track| Session { at, track })
        })
        .collect();

    let sleep_rows: Vec<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT bedtime, wake_time FROM sleep_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND wake_time > $2 AND wake_time <= $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_all(pool)
    .await?;
    let sleeps = sleep_rows
        .into_iter()
        .map(|(start, end)| SleepInterval { start, end })
        .collect();

    let food: Vec<(DateTime<Utc>, f64)> = sqlx::query_as(
        "SELECT logged_at, protein_g FROM food_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND logged_at > $2 AND logged_at <= $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_all(pool)
    .await?;
    let protein = food
        .into_iter()
        .map(|(at, grams)| ProteinEntry { at, grams })
        .collect();

    let day_start = start_of_day(now);
    let secs: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(duration_s)::bigint FROM workouts \
         WHERE user_id = $1 AND deleted_at IS NULL AND started_at >= $2 AND started_at <= $3",
    )
    .bind(user_id)
    .bind(day_start)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let steps: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(value)::float8::bigint FROM health_samples \
         WHERE user_id = $1 AND category = 'steps' AND date = $2",
    )
    .bind(user_id)
    .bind(now.date_naive())
    .fetch_one(pool)
    .await?;
    let active_minutes_today = secs.unwrap_or(0) / 60 + steps.unwrap_or(0).max(0) / 100;

    Ok(Inputs {
        sessions,
        protein,
        sleeps,
        active_minutes_today,
    })
}

/// Full progression for a user, including personal bests set in the last 24 h.
pub async fn for_user(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Progression, sqlx::Error> {
    let inputs = load_inputs(pool, user_id, now).await?;
    let mut p = compute(&inputs, now);
    p.new_personal_bests = records::recent_personal_bests(pool, user_id, now).await?;
    Ok(p)
}

async fn get_progression(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Progression>, AppError> {
    if !state.config.feature("stats") {
        return Err(AppError::NotFound);
    }
    Ok(Json(for_user(&state.pool, user.0, Utc::now()).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 8, 0, 0).unwrap()
    }
    fn h(n: i64) -> Duration {
        Duration::hours(n)
    }
    fn night(end: DateTime<Utc>, hours: i64) -> SleepInterval {
        SleepInterval {
            start: end - h(hours),
            end,
        }
    }
    fn protein(at: DateTime<Utc>, g: f64) -> ProteinEntry {
        ProteinEntry { at, grams: g }
    }
    /// A recent night so the rest rule is skipped no matter what (no sleeps => skipped).
    fn strength(
        sessions: &[DateTime<Utc>],
        p: &[ProteinEntry],
        s: &[SleepInterval],
        now: DateTime<Utc>,
    ) -> WindowTrack {
        window_track(Track::Strength, sessions, p, s, now)
    }
    fn cardio(
        sessions: &[DateTime<Utc>],
        p: &[ProteinEntry],
        s: &[SleepInterval],
        now: DateTime<Utc>,
    ) -> WindowTrack {
        window_track(Track::Cardio, sessions, p, s, now)
    }

    #[test]
    fn level_thresholds() {
        for (p, l) in [
            (0, 1),
            (3, 1),
            (4, 2),
            (11, 2),
            (12, 3),
            (23, 3),
            (24, 4),
            (100, 4),
        ] {
            assert_eq!(level_info(p).level, l, "points {p}");
        }
        assert_eq!(level_info(-5).points, 0);
    }

    #[test]
    fn progress_and_value() {
        let z = level_info(0);
        assert_eq!((z.progress, z.value), (0.0, 0));
        let two = level_info(2);
        assert!((two.progress - 0.5).abs() < 1e-9);
        assert_eq!(two.value, 13); // 0 + 25 * 0.5 rounded
        let l2 = level_info(4);
        assert_eq!((l2.progress, l2.value), (0.0, 25));
        let l3 = level_info(12);
        assert_eq!(l3.value, 50);
        let mid = level_info(18);
        assert!((mid.progress - 0.5).abs() < 1e-9);
        assert_eq!(mid.value, 63);
        let top = level_info(24);
        assert_eq!((top.progress, top.value), (1.0, 100));
        assert_eq!(level_info(500).value, 100);
    }

    #[test]
    fn no_sessions_means_open_window_and_no_closing_time() {
        let w = strength(&[], &[], &[], t0());
        assert_eq!(w.state, WindowState::Open);
        assert_eq!((w.opens_at, w.closes_at), (None, None));
        assert_eq!(w.fraction, 1.0);
        assert_eq!(w.level.points, 0);
        assert!(!w.protein_bonus);
    }

    #[test]
    fn first_session_scores_and_starts_recovery() {
        let w = strength(&[t0()], &[], &[], t0() + h(5));
        assert_eq!(w.state, WindowState::Recovering);
        assert_eq!(w.level.points, 1);
        assert_eq!(w.opens_at, Some(t0() + h(20)));
        assert_eq!(w.closes_at, Some(t0() + h(68)));
        assert!((w.fraction - 0.25).abs() < 1e-9);
    }

    #[test]
    fn recovery_is_20h_strength_and_12h_cardio() {
        let s = strength(&[t0()], &[], &[], t0() + h(19));
        assert_eq!(s.state, WindowState::Recovering);
        let s = strength(&[t0()], &[], &[], t0() + h(20));
        assert_eq!(s.state, WindowState::Open);
        let c = cardio(&[t0()], &[], &[], t0() + h(11));
        assert_eq!(c.state, WindowState::Recovering);
        let c = cardio(&[t0()], &[], &[], t0() + h(12));
        assert_eq!(c.state, WindowState::Open);
        assert_eq!(c.opens_at, Some(t0() + h(12)));
    }

    #[test]
    fn protein_halves_recovery() {
        let p = [protein(t0() + h(1), 30.0)];
        let w = strength(&[t0()], &p, &[], t0() + h(10));
        assert_eq!(w.state, WindowState::Open);
        assert!(w.protein_bonus);
        assert_eq!(w.opens_at, Some(t0() + h(10)));
        let c = cardio(&[t0()], &p, &[], t0() + h(5));
        assert_eq!(c.state, WindowState::Recovering);
        let c = cardio(&[t0()], &p, &[], t0() + h(6));
        assert_eq!(c.state, WindowState::Open);
    }

    #[test]
    fn protein_bonus_edges() {
        let p = |at: DateTime<Utc>, g| [protein(at, g)];
        assert!(protein_bonus(t0(), &p(t0() + h(2), 25.0))); // inclusive
        assert!(protein_bonus(t0(), &p(t0(), 25.0)));
        assert!(!protein_bonus(
            t0(),
            &p(t0() + h(2) + Duration::seconds(1), 40.0)
        ));
        assert!(!protein_bonus(t0(), &p(t0() - Duration::seconds(1), 40.0)));
        assert!(!protein_bonus(t0(), &p(t0() + h(1), 24.9)));
        // several entries add up
        assert!(protein_bonus(
            t0(),
            &[protein(t0() + h(1), 10.0), protein(t0() + h(2), 15.0)]
        ));
    }

    #[test]
    fn needs_rest_until_a_six_hour_night() {
        // A short nap after the session is not enough, a recent night exists so the rule applies.
        let nap = night(t0() + h(3), 2);
        let w = strength(&[t0()], &[], &[nap], t0() + h(30));
        assert_eq!(w.state, WindowState::NeedsRest);
        assert_eq!((w.opens_at, w.closes_at), (None, None));
        assert_eq!(w.fraction, 1.0);
        // a 5h59m night is still not enough
        let short = SleepInterval {
            start: t0() + h(12),
            end: t0() + h(12) + Duration::minutes(359),
        };
        let w = strength(&[t0()], &[], &[short], t0() + h(30));
        assert_eq!(w.state, WindowState::NeedsRest);
        // a 6h night ending after the session opens the window (recovery already over)
        let ok = night(t0() + h(22), 6);
        let w = strength(&[t0()], &[], &[ok], t0() + h(30));
        assert_eq!(w.state, WindowState::Open);
        assert_eq!(w.opens_at, Some(t0() + h(22)));
        assert_eq!(w.closes_at, Some(t0() + h(70)));
    }

    #[test]
    fn night_before_the_session_does_not_count() {
        let before = night(t0() - Duration::minutes(10), 8);
        let w = strength(&[t0()], &[], &[before], t0() + h(30));
        assert_eq!(w.state, WindowState::NeedsRest);
    }

    #[test]
    fn rest_rule_skipped_without_recent_sleep() {
        assert!(!rest_rule_active(&[], t0()));
        // sleep older than 7 days does not activate the rule
        let old = night(t0() - Duration::days(8), 8);
        assert!(!rest_rule_active(&[old], t0()));
        let w = strength(&[t0() - Duration::days(1)], &[], &[old], t0());
        assert_eq!(w.state, WindowState::Open);
        // with a recent night the same history would need rest
        let w = strength(
            &[t0() - Duration::days(1)],
            &[],
            &[old, night(t0() - h(30), 3)],
            t0(),
        );
        assert_eq!(w.state, WindowState::NeedsRest);
        let recent = night(t0() - h(2), 8);
        assert!(rest_rule_active(&[recent], t0()));
    }

    #[test]
    fn recovering_with_night_still_waits_for_recovery_end() {
        // The night ends at +8h but recovery lasts until +20h.
        let n = night(t0() + h(8), 8);
        let w = strength(&[t0()], &[], &[n], t0() + h(15));
        assert_eq!(w.state, WindowState::Recovering);
        assert_eq!(w.opens_at, Some(t0() + h(20)));
    }

    #[test]
    fn open_window_drains_from_one_to_zero() {
        let open = t0() + h(20);
        let at = |x: Duration| strength(&[t0()], &[], &[], open + x).fraction;
        assert!((at(Duration::zero()) - 1.0).abs() < 1e-9);
        assert!((at(h(24)) - 0.5).abs() < 1e-9);
        assert!((at(h(36)) - 0.25).abs() < 1e-9);
        assert!(at(h(48)).abs() < 1e-9);
        let w = strength(&[t0()], &[], &[], open + h(48));
        assert_eq!(w.state, WindowState::Open); // closes_at is inclusive
    }

    #[test]
    fn on_time_session_scores_and_restarts() {
        let second = t0() + h(30);
        let w = strength(&[t0(), second], &[], &[], second + h(1));
        assert_eq!(w.level.points, 2);
        assert_eq!(w.state, WindowState::Recovering);
        assert_eq!(w.opens_at, Some(second + h(20)));
    }

    #[test]
    fn training_while_recovering_does_not_count() {
        let w = strength(&[t0(), t0() + h(5), t0() + h(10)], &[], &[], t0() + h(12));
        assert_eq!(w.level.points, 1);
        assert_eq!(w.opens_at, Some(t0() + h(20))); // cycle not restarted
    }

    #[test]
    fn training_while_needs_rest_does_not_count() {
        let nap = night(t0() + h(3), 2);
        let w = strength(&[t0(), t0() + h(30)], &[], &[nap], t0() + h(31));
        assert_eq!(w.level.points, 1);
        assert_eq!(w.state, WindowState::NeedsRest);
    }

    #[test]
    fn missed_window_costs_a_point_and_state_is_missed() {
        let sessions = [
            t0(),
            t0() + h(30),
            t0() + h(60),
            t0() + h(90),
            t0() + h(120),
        ];
        // 5 on-time sessions => 5 points; last at +120h, window closes at +188h.
        let w = strength(&sessions, &[], &[], t0() + h(150));
        assert_eq!(w.level.points, 5);
        let w = strength(&sessions, &[], &[], t0() + h(189));
        assert_eq!(w.state, WindowState::Missed);
        assert_eq!(w.level.points, 4);
        assert_eq!(w.fraction, 0.0);
        assert_eq!(w.closes_at, Some(t0() + h(188)));
    }

    #[test]
    fn repeated_misses_keep_costing_points_floored_at_zero() {
        let sessions = [
            t0(),
            t0() + h(30),
            t0() + h(60),
            t0() + h(90),
            t0() + h(120),
        ];
        let close = t0() + h(188);
        let at = |x: i64| strength(&sessions, &[], &[], close + h(x)).level.points;
        assert_eq!(at(1), 4);
        assert_eq!(at(47), 4);
        assert_eq!(at(48), 3);
        assert_eq!(at(96), 2);
        assert_eq!(at(144), 1);
        assert_eq!(at(192), 0);
        assert_eq!(at(1000), 0);
    }

    #[test]
    fn missed_windows_count() {
        let c = t0();
        assert_eq!(missed_windows(c, c), 0);
        assert_eq!(missed_windows(c, c - h(1)), 0);
        assert_eq!(missed_windows(c, c + h(1)), 1);
        assert_eq!(missed_windows(c, c + h(48)), 2);
        assert_eq!(missed_windows(c, c + h(95)), 2);
        assert_eq!(missed_windows(c, c + h(96)), 3);
    }

    #[test]
    fn session_after_a_miss_pays_the_penalty_and_restarts_without_a_point() {
        let sessions = [
            t0(),
            t0() + h(30),
            t0() + h(60),
            t0() + h(90),
            t0() + h(120),
        ];
        let close = t0() + h(188);
        // returns 10h after the close: one miss, no point for the comeback itself.
        let back = close + h(10);
        let mut s = sessions.to_vec();
        s.push(back);
        let w = strength(&s, &[], &[], back + h(1));
        assert_eq!(w.level.points, 4);
        assert_eq!(w.state, WindowState::Recovering);
        assert_eq!(w.opens_at, Some(back + h(20)));
        // returning after two missed windows costs 2
        let back2 = close + h(50);
        let mut s = sessions.to_vec();
        s.push(back2);
        assert_eq!(strength(&s, &[], &[], back2 + h(1)).level.points, 3);
    }

    #[test]
    fn demotion_when_points_fall_below_a_threshold() {
        // 4 points = level 2
        let s: Vec<_> = (0..4).map(|i| t0() + h(30 * i)).collect();
        let last_close = t0() + h(90 + 68);
        let w = strength(&s, &[], &[], last_close - h(1));
        assert_eq!((w.level.points, w.level.level), (4, 2));
        let w = strength(&s, &[], &[], last_close + h(1));
        assert_eq!((w.level.points, w.level.level), (3, 1));
    }

    #[test]
    fn future_sessions_are_ignored_and_input_order_does_not_matter() {
        let a = strength(&[t0() + h(30), t0(), t0() + h(500)], &[], &[], t0() + h(31));
        assert_eq!(a.level.points, 2);
        let b = strength(&[t0(), t0() + h(30)], &[], &[], t0() + h(31));
        assert_eq!(a, b);
    }

    #[test]
    fn focus_gains_once_per_distinct_day_and_never_loses() {
        let day = |d: u32, hh: u32| Utc.with_ymd_and_hms(2026, 6, d, hh, 0, 0).unwrap();
        let now = day(20, 12);
        let f = focus_track(&[day(1, 8), day(1, 20), day(2, 9), day(2, 10)], now);
        assert_eq!(f.level.points, 2);
        // long idle never drops it
        let later = day(1, 8) + Duration::days(300);
        assert_eq!(focus_track(&[day(1, 8), day(2, 9)], later).level.points, 2);
        // future sessions ignored
        assert_eq!(focus_track(&[day(25, 8)], now).level.points, 0);
        // 4 distinct days reach level 2
        let four = [day(1, 8), day(2, 8), day(3, 8), day(4, 8)];
        assert_eq!(focus_track(&four, now).level.level, 2);
    }

    #[test]
    fn ring_closes_at_thirty_minutes() {
        assert!(!ring(29).closed);
        assert!(ring(30).closed);
        assert!(ring(500).closed);
        assert_eq!(ring(-4).minutes, 0);
        assert_eq!(ring(10).goal, 30);
    }

    fn quiet() -> WindowTrack {
        strength(&[], &[], &[], t0())
    }
    fn recovering() -> WindowTrack {
        strength(&[t0()], &[], &[], t0() + h(1))
    }

    #[test]
    fn hint_ring_closed() {
        let r = ring(40);
        let hs = hints(&r, &recovering(), &recovering(), &[], &[], t0());
        assert!(hs.contains(&MSG_RING_DONE.to_string()));
        let hs = hints(&ring(5), &recovering(), &recovering(), &[], &[], t0());
        assert!(!hs.contains(&MSG_RING_DONE.to_string()));
    }

    #[test]
    fn hints_after_workout_and_protein_tip() {
        let s = [Session {
            at: t0(),
            track: Track::Strength,
        }];
        let now = t0() + h(1);
        let hs = hints(&ring(0), &recovering(), &recovering(), &s, &[], now);
        assert_eq!(hs, vec![MSG_WORKOUT_DONE, MSG_PROTEIN]);
        // protein already logged: no tip
        let p = [protein(t0() + Duration::minutes(30), 30.0)];
        let hs = hints(&ring(0), &recovering(), &recovering(), &s, &p, now);
        assert_eq!(hs, vec![MSG_WORKOUT_DONE]);
        // more than 2h later: nothing
        let hs = hints(&ring(0), &recovering(), &recovering(), &s, &p, t0() + h(3));
        assert!(hs.is_empty());
        // focus sessions do not trigger the post-workout message
        let f = [Session {
            at: t0(),
            track: Track::Focus,
        }];
        let hs = hints(&ring(0), &recovering(), &recovering(), &f, &[], now);
        assert!(hs.is_empty());
    }

    #[test]
    fn fuel_hint_when_window_open_and_not_fed() {
        let now = t0() + h(30);
        let open = strength(&[t0()], &[], &[], now);
        assert_eq!(open.state, WindowState::Open);
        let hs = hints(&ring(0), &open, &recovering(), &[], &[], now);
        assert_eq!(hs, vec![MSG_FUEL]);
        // food logged within 4h removes it
        let fed = [protein(now - h(3), 0.0)];
        assert!(hints(&ring(0), &open, &recovering(), &[], &fed, now).is_empty());
        // food 4h ago is old enough
        let old = [protein(now - h(4), 20.0)];
        assert_eq!(
            hints(&ring(0), &open, &recovering(), &[], &old, now),
            vec![MSG_FUEL]
        );
        // no window open: no hint
        assert!(hints(&ring(0), &recovering(), &recovering(), &[], &[], now).is_empty());
        // brand new user: windows are open
        assert_eq!(
            hints(&ring(0), &quiet(), &quiet(), &[], &[], now),
            vec![MSG_FUEL]
        );
    }

    #[test]
    fn tracks_for_sets() {
        assert_eq!(tracks_for(3, 0, 0), vec![Track::Strength]);
        assert_eq!(tracks_for(0, 2, 0), vec![Track::Cardio]);
        assert_eq!(tracks_for(0, 0, 0), vec![Track::Cardio]);
        assert_eq!(tracks_for(0, 0, 1), vec![Track::Focus]);
        assert_eq!(
            tracks_for(2, 1, 1),
            vec![Track::Strength, Track::Cardio, Track::Focus]
        );
    }

    #[test]
    fn compute_assembles_tracks() {
        let inputs = Inputs {
            sessions: vec![
                Session {
                    at: t0(),
                    track: Track::Strength,
                },
                Session {
                    at: t0(),
                    track: Track::Focus,
                },
            ],
            protein: vec![],
            sleeps: vec![],
            active_minutes_today: 45,
        };
        let p = compute(&inputs, t0() + h(1));
        assert_eq!(p.tracks.strength.level.points, 1);
        assert_eq!(p.tracks.cardio.level.points, 0);
        assert_eq!(p.tracks.focus.level.points, 1);
        assert!(p.ring.closed);
        assert!(p.new_personal_bests.is_empty());
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["tracks"]["strength"]["state"], "recovering");
        assert_eq!(json["tracks"]["strength"]["level"], 1);
        assert_eq!(json["tracks"]["focus"]["points"], 1);
        assert!(json["tracks"]["focus"].get("state").is_none());
        assert_eq!(json["ring"]["goal"], 30);
        assert_eq!(
            serde_json::to_value(WindowState::NeedsRest).unwrap(),
            "needs_rest"
        );
    }
}

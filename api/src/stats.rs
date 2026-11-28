//! Game-style stats (Strength, Cardio, Focus). See docs/stats.md and
//! docs/progression.md: levels and values now come from the progression model.
//!
//! `current_stats` loads the user's history, runs the pure progression model
//! and upserts the daily snapshot. Every query is scoped to the given user id.

use crate::{
    auth::AuthUser,
    error::AppError,
    progression::{self, LevelInfo, Progression},
    AppState,
};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/stats", get(get_stats))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Stat {
    pub value: u8,
    pub level: u8,
    pub progress: f64,
}

impl From<LevelInfo> for Stat {
    fn from(l: LevelInfo) -> Self {
        Stat {
            value: l.value,
            level: l.level,
            progress: l.progress,
        }
    }
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
        let z = Stat::from(progression::level_info(0));
        Stats {
            strength: z,
            cardio: z,
            focus: z,
        }
    }
}

impl From<&Progression> for Stats {
    fn from(p: &Progression) -> Self {
        Stats {
            strength: p.tracks.strength.level.into(),
            cardio: p.tracks.cardio.level.into(),
            focus: p.tracks.focus.level.into(),
        }
    }
}

/// Loads history, computes stats and upserts today's snapshot.
/// `None` when the user has no workouts in the loaded history.
pub async fn current_stats(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<Stats>, sqlx::Error> {
    let inputs = progression::load_inputs(pool, user_id, now).await?;
    if inputs.sessions.is_empty() {
        return Ok(None);
    }
    let stats = Stats::from(&progression::compute(&inputs, now));
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
    use crate::progression::{compute, Inputs, Session, Track};
    use chrono::{Duration, TimeZone};

    #[test]
    fn zero_stats_are_level_one() {
        let z = Stats::zero();
        for s in [z.strength, z.cardio, z.focus] {
            assert_eq!((s.value, s.level), (0, 1));
            assert_eq!(s.progress, 0.0);
        }
    }

    #[test]
    fn stats_follow_the_progression_model() {
        let t0 = Utc.with_ymd_and_hms(2026, 6, 1, 8, 0, 0).unwrap();
        let sessions = (0..4)
            .map(|i| Session {
                at: t0 + Duration::hours(30 * i),
                track: Track::Strength,
            })
            .collect();
        let inputs = Inputs {
            sessions,
            ..Inputs::default()
        };
        let p = compute(&inputs, t0 + Duration::hours(100));
        let s = Stats::from(&p);
        assert_eq!((s.strength.level, s.strength.value), (2, 25));
        assert_eq!(s.cardio, Stats::zero().cardio);
        assert_eq!(s.focus, Stats::zero().focus);
    }
}

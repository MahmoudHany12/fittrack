//! Friends-only leaderboard (v2, feature flag `leaderboard`). See docs/spec.md.
//!
//! Privacy model:
//! - Strictly opt-in (`leaderboard_optin`); no row means not opted in.
//! - The board contains the caller plus friends who ALSO opted in. A caller who
//!   has not opted in gets 403. Opting out takes effect immediately (every read
//!   re-checks) and unfriending removes both sides from each other's boards.
//! - Only display name, stat values and levels are exposed: never email.
//! - Every handler first checks the flag (`LeaderboardEnabled`) so routes are
//!   404 when the feature is off, even before authentication.

use crate::{
    auth::AuthUser,
    error::AppError,
    extract::ValidatedJson,
    stats::{self, Stat},
    AppState,
};
use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/leaderboard/optin", get(get_optin).put(set_optin))
        .route("/leaderboard", get(board))
}

/// Extractor that answers 404 when the `leaderboard` flag is off. Must be the
/// first handler argument so it runs before authentication.
pub struct LeaderboardEnabled;

impl FromRequestParts<AppState> for LeaderboardEnabled {
    type Rejection = AppError;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.config.feature("leaderboard") {
            Ok(Self)
        } else {
            Err(AppError::NotFound)
        }
    }
}

#[derive(Deserialize, Validate)]
pub struct OptInBody {
    opted_in: bool,
}

async fn is_opted_in(pool: &sqlx::PgPool, user: Uuid) -> Result<bool, AppError> {
    let v: Option<bool> =
        sqlx::query_scalar("SELECT opted_in FROM leaderboard_optin WHERE user_id = $1")
            .bind(user)
            .fetch_optional(pool)
            .await?;
    Ok(v.unwrap_or(false))
}

async fn get_optin(
    _flag: LeaderboardEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({ "opted_in": is_opted_in(&state.pool, me).await? }),
    ))
}

async fn set_optin(
    _flag: LeaderboardEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<OptInBody>,
) -> Result<Json<Value>, AppError> {
    sqlx::query(
        "INSERT INTO leaderboard_optin (user_id, opted_in) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET opted_in = $2, updated_at = now()",
    )
    .bind(me)
    .bind(body.opted_in)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "opted_in": body.opted_in })))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub user_id: Uuid,
    pub display_name: String,
    pub is_me: bool,
    pub rank: usize,
    pub strength: Option<Stat>,
    pub cardio: Option<Stat>,
    pub focus: Option<Stat>,
    pub total: Option<u32>,
}

/// Sort key: users with stats first by total desc, then name, then id.
/// Users without stats rank last (still ordered by name, id).
pub fn rank(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        b.total
            .is_some()
            .cmp(&a.total.is_some())
            .then_with(|| b.total.cmp(&a.total))
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
            .then_with(|| a.display_name.cmp(&b.display_name))
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
    for (i, e) in entries.iter_mut().enumerate() {
        e.rank = i + 1;
    }
    entries
}

async fn board(
    _flag: LeaderboardEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Entry>>, AppError> {
    if !is_opted_in(&state.pool, me).await? {
        return Err(AppError::Forbidden);
    }
    // The caller plus friends who opted in. Friendship is the only path in.
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, COALESCE(NULLIF(btrim(p.display_name), ''), 'Friend')
         FROM (SELECT $1::uuid AS id
               UNION
               SELECT CASE WHEN f.user_a = $1 THEN f.user_b ELSE f.user_a END
               FROM friendships f WHERE f.user_a = $1 OR f.user_b = $1) u
         JOIN leaderboard_optin o ON o.user_id = u.id AND o.opted_in
         LEFT JOIN profiles p ON p.user_id = u.id",
    )
    .bind(me)
    .fetch_all(&state.pool)
    .await?;
    let now = Utc::now();
    let mut entries = Vec::with_capacity(rows.len());
    for (id, display_name) in rows {
        let s = stats::current_stats(&state.pool, id, now)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        entries.push(Entry {
            user_id: id,
            display_name,
            is_me: id == me,
            rank: 0,
            strength: s.map(|s| s.strength),
            cardio: s.map(|s| s.cardio),
            focus: s.map(|s| s.focus),
            total: s.map(|s| {
                u32::from(s.strength.value) + u32::from(s.cardio.value) + u32::from(s.focus.value)
            }),
        });
    }
    Ok(Json(rank(entries)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, total: Option<u32>) -> Entry {
        Entry {
            user_id: Uuid::new_v4(),
            display_name: name.into(),
            is_me: false,
            rank: 0,
            strength: None,
            cardio: None,
            focus: None,
            total,
        }
    }

    #[test]
    fn ranks_by_total_then_name_with_none_last() {
        let r = rank(vec![
            e("Zed", None),
            e("Bob", Some(50)),
            e("Ann", Some(50)),
            e("Cy", Some(90)),
            e("Abe", None),
        ]);
        let names: Vec<_> = r.iter().map(|x| x.display_name.as_str()).collect();
        assert_eq!(names, ["Cy", "Ann", "Bob", "Abe", "Zed"]);
        assert_eq!(
            r.iter().map(|x| x.rank).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn ties_fall_back_to_id() {
        let (a, b) = (e("Same", Some(10)), e("Same", Some(10)));
        let lo = a.user_id.min(b.user_id);
        assert_eq!(rank(vec![a, b])[0].user_id, lo);
    }
}

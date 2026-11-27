//! Friends (v2, feature flag `friends`). See docs/spec.md. Routes use full paths.
//!
//! Privacy model:
//! - Friendship is an invite-code handshake; stored once as a normalised pair.
//! - Sharing is opt-in per friend and covers the WEEKLY SUMMARY ONLY. Nothing
//!   else (email, workouts, food, sleep details) is ever exposed to a friend;
//!   friends see just a display name.
//! - Not friends -> 404 (existence is not revealed); friends without an active
//!   grant -> 403. Revoking takes effect immediately (checked on every read).
//! - Every handler first checks the flag (`FriendsEnabled`) so routes are 404
//!   when the feature is off, even before authentication.

use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::{FromRequestParts, Path, State},
    http::{request::Parts, StatusCode},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Datelike, Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/friends", get(list_friends))
        .route("/friends/invites", post(create_invite))
        .route("/friends/invites/{code}/accept", post(accept_invite))
        .route("/friends/{user_id}", delete(unfriend))
        .route("/friends/{user_id}/share", put(set_share))
        .route("/friends/{user_id}/summary", get(friend_summary))
}

/// 32 unambiguous characters (no 0/O, 1/I): 32 divides 256, so masking a
/// random byte is unbiased.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 10;
const INVITE_TTL_DAYS: i64 = 7;
const MAX_LIVE_INVITES: i64 = 20;

/// Extractor that answers 404 when the `friends` flag is off. Must be the first
/// handler argument so it runs before authentication.
pub struct FriendsEnabled;

impl FromRequestParts<AppState> for FriendsEnabled {
    type Rejection = AppError;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.config.feature("friends") {
            Ok(Self)
        } else {
            Err(AppError::NotFound)
        }
    }
}

fn generate_code() -> String {
    let mut bytes = [0u8; CODE_LEN];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[(b & 31) as usize] as char)
        .collect()
}

/// Normalise user input; anything malformed can never match a stored code.
fn normalise_code(raw: &str) -> Option<String> {
    let code = raw.trim().to_ascii_uppercase();
    (code.len() == CODE_LEN && code.bytes().all(|b| ALPHABET.contains(&b))).then_some(code)
}

/// Normalised (smaller, larger) pair, as stored.
fn pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

async fn are_friends(pool: &PgPool, a: Uuid, b: Uuid) -> Result<bool, AppError> {
    let (lo, hi) = pair(a, b);
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM friendships WHERE user_a = $1 AND user_b = $2)",
    )
    .bind(lo)
    .bind(hi)
    .fetch_one(pool)
    .await?)
}

// ---------- invites ----------

#[derive(Serialize)]
pub struct InviteResponse {
    code: String,
    link: String,
    expires_at: DateTime<Utc>,
}

async fn create_invite(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<InviteResponse>), AppError> {
    let mut tx = state.pool.begin().await?;
    // Serialise per user so the live-invite cap cannot be raced.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(me)
        .execute(&mut *tx)
        .await?;
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invites WHERE owner = $1 AND used_by IS NULL AND expires_at > now()",
    )
    .bind(me)
    .fetch_one(&mut *tx)
    .await?;
    if live >= MAX_LIVE_INVITES {
        return Err(AppError::Conflict("Too many open invites"));
    }
    let expires_at = Utc::now() + Duration::days(INVITE_TTL_DAYS);
    for _ in 0..5 {
        let code = generate_code();
        let inserted = sqlx::query(
            "INSERT INTO invites (code, owner, expires_at) VALUES ($1, $2, $3)
             ON CONFLICT (code) DO NOTHING",
        )
        .bind(&code)
        .bind(me)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 1 {
            tx.commit().await?;
            let link = format!("fittrack://invite/{code}");
            return Ok((
                StatusCode::CREATED,
                Json(InviteResponse {
                    code,
                    link,
                    expires_at,
                }),
            ));
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not generate a unique invite code"
    )))
}

async fn accept_invite(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Value>, AppError> {
    let code = normalise_code(&raw).ok_or(AppError::NotFound)?;
    let mut tx = state.pool.begin().await?;
    // Expired, used and unknown codes are indistinguishable: all 404.
    let owner: Uuid = sqlx::query_scalar(
        "SELECT owner FROM invites
         WHERE code = $1 AND used_by IS NULL AND expires_at > now()
         FOR UPDATE",
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if owner == me {
        return Err(AppError::BadRequest(
            "You cannot accept your own invite".into(),
        ));
    }
    let (lo, hi) = pair(owner, me);
    let inserted = sqlx::query(
        "INSERT INTO friendships (user_a, user_b) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(lo)
    .bind(hi)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Err(AppError::Conflict("Already friends"));
    }
    sqlx::query("UPDATE invites SET used_by = $2, used_at = now() WHERE code = $1")
        .bind(&code)
        .bind(me)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "friend_id": owner })))
}

// ---------- friends ----------

#[derive(Serialize, sqlx::FromRow)]
pub struct FriendRow {
    user_id: Uuid,
    display_name: String,
    sharing_with_me: bool,
    i_share: bool,
}

async fn list_friends(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<FriendRow>>, AppError> {
    let rows = sqlx::query_as::<_, FriendRow>(
        "SELECT f.other AS user_id,
                COALESCE(NULLIF(btrim(p.display_name), ''), 'Friend') AS display_name,
                EXISTS (SELECT 1 FROM share_grants g
                        WHERE g.owner = f.other AND g.viewer = $1 AND g.revoked_at IS NULL)
                    AS sharing_with_me,
                EXISTS (SELECT 1 FROM share_grants g
                        WHERE g.owner = $1 AND g.viewer = f.other AND g.revoked_at IS NULL)
                    AS i_share
         FROM (SELECT CASE WHEN user_a = $1 THEN user_b ELSE user_a END AS other, created_at
               FROM friendships WHERE user_a = $1 OR user_b = $1) f
         LEFT JOIN profiles p ON p.user_id = f.other
         ORDER BY lower(COALESCE(p.display_name, '')), f.created_at",
    )
    .bind(me)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn unfriend(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(other): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let (lo, hi) = pair(me, other);
    let mut tx = state.pool.begin().await?;
    let removed = sqlx::query("DELETE FROM friendships WHERE user_a = $1 AND user_b = $2")
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(AppError::NotFound);
    }
    // Unfriending revokes sharing in both directions.
    sqlx::query(
        "UPDATE share_grants SET revoked_at = now()
         WHERE revoked_at IS NULL
           AND ((owner = $1 AND viewer = $2) OR (owner = $2 AND viewer = $1))",
    )
    .bind(me)
    .bind(other)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- sharing ----------

#[derive(Deserialize, Validate)]
pub struct ShareBody {
    enabled: bool,
}

async fn set_share(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(viewer): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<ShareBody>,
) -> Result<Json<Value>, AppError> {
    // Only between friends (also rejects yourself).
    if !are_friends(&state.pool, me, viewer).await? {
        return Err(AppError::NotFound);
    }
    if body.enabled {
        sqlx::query(
            "INSERT INTO share_grants (owner, viewer) VALUES ($1, $2)
             ON CONFLICT (owner, viewer) DO UPDATE
             SET granted_at = CASE WHEN share_grants.revoked_at IS NULL
                                   THEN share_grants.granted_at ELSE now() END,
                 revoked_at = NULL",
        )
        .bind(me)
        .bind(viewer)
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE share_grants SET revoked_at = now()
             WHERE owner = $1 AND viewer = $2 AND revoked_at IS NULL",
        )
        .bind(me)
        .bind(viewer)
        .execute(&state.pool)
        .await?;
    }
    Ok(Json(json!({ "enabled": body.enabled })))
}

/// 403 unless `owner` has an active weekly-summary grant to `viewer`.
async fn require_grant(pool: &PgPool, owner: Uuid, viewer: Uuid) -> Result<(), AppError> {
    let granted: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM share_grants
                        WHERE owner = $1 AND viewer = $2 AND revoked_at IS NULL)",
    )
    .bind(owner)
    .bind(viewer)
    .fetch_one(pool)
    .await?;
    if granted {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Monday (UTC) of the week containing `now`.
fn week_start(now: DateTime<Utc>) -> chrono::NaiveDate {
    let d = now.date_naive();
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

/// Loads the owner's weekly summary.
/// TODO: wire in `summary::weekly_summary(pool, user_id, week_start)` once that
/// lands; until then this returns a placeholder. It must only ever return the
/// weekly summary (never raw logs).
async fn load_summary(
    _pool: &PgPool,
    user_id: Uuid,
    week_start: chrono::NaiveDate,
) -> Result<Value, AppError> {
    Ok(json!({
        "user_id": user_id,
        "week_start": week_start,
        "placeholder": true,
    }))
}

async fn friend_summary(
    _flag: FriendsEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(owner): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if !are_friends(&state.pool, me, owner).await? {
        return Err(AppError::NotFound);
    }
    require_grant(&state.pool, owner, me).await?;
    let summary = load_summary(&state.pool, owner, week_start(Utc::now())).await?;
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM profiles WHERE user_id = $1")
            .bind(owner)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    Ok(Json(json!({
        "user_id": owner,
        "display_name": display_name.unwrap_or_else(|| "Friend".into()),
        "summary": summary,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_well_formed() {
        for _ in 0..200 {
            let c = generate_code();
            assert_eq!(c.len(), CODE_LEN);
            assert!(c.bytes().all(|b| ALPHABET.contains(&b)));
        }
        assert_ne!(generate_code(), generate_code());
    }

    #[test]
    fn normalise_rejects_bad_codes() {
        assert_eq!(normalise_code(" abcdefghjk "), Some("ABCDEFGHJK".into()));
        assert_eq!(normalise_code("ABCDEFGHJ"), None);
        assert_eq!(normalise_code("ABCDEFGHJ0"), None);
        assert_eq!(normalise_code("ABCDEFGHJI"), None);
    }

    #[test]
    fn pair_is_ordered() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(pair(a, b), pair(b, a));
        assert!(pair(a, b).0 < pair(a, b).1);
    }
}

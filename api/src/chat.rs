//! Chat (v2, feature flag `chat`). See docs/spec.md. Routes use full paths.
//!
//! Privacy model:
//! - Only FRIENDS can chat. Not friends (never were, or unfriended) -> 404 on
//!   every route, so existence is not revealed and old history is unreachable.
//! - History is kept on unfriend but is only served while the friendship exists.
//! - Every query is scoped to the caller's own conversation with that friend;
//!   third parties can never read or delete messages.
//! - Every handler first checks the flag (`ChatEnabled`) so routes are 404
//!   when the feature is off, even before authentication.

use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/chat", get(list_conversations))
        .route("/chat/{friend_id}", get(get_thread).post(send_message))
        .route("/chat/{friend_id}/messages/{id}", delete(delete_message))
}

const MAX_BODY_CHARS: usize = 2000;
const MAX_PER_MINUTE: i64 = 30;
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;
const PREVIEW_CHARS: i32 = 80;

/// Extractor that answers 404 when the `chat` flag is off. Must be the first
/// handler argument so it runs before authentication.
pub struct ChatEnabled;

impl FromRequestParts<AppState> for ChatEnabled {
    type Rejection = AppError;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.config.feature("chat") {
            Ok(Self)
        } else {
            Err(AppError::NotFound)
        }
    }
}

async fn require_friend(pool: &PgPool, me: Uuid, other: Uuid) -> Result<(), AppError> {
    let (lo, hi) = if me < other { (me, other) } else { (other, me) };
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM friendships WHERE user_a = $1 AND user_b = $2)",
    )
    .bind(lo)
    .bind(hi)
    .fetch_one(pool)
    .await?;
    if ok {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// Trim and validate a message body: 1..=2000 chars, no control chars but newline.
fn clean_body(raw: &str) -> Result<String, AppError> {
    let body = raw.trim();
    let n = body.chars().count();
    if n == 0 {
        return Err(AppError::BadRequest("Message cannot be empty".into()));
    }
    if n > MAX_BODY_CHARS {
        return Err(AppError::BadRequest(format!(
            "Message is too long (max {MAX_BODY_CHARS} characters)"
        )));
    }
    if body.chars().any(|c| c.is_control() && c != '\n') {
        return Err(AppError::BadRequest(
            "Message contains invalid characters".into(),
        ));
    }
    Ok(body.to_string())
}

// ---------- conversation list ----------

#[derive(Serialize, sqlx::FromRow)]
pub struct Conversation {
    friend_id: Uuid,
    display_name: String,
    last_message: String,
    last_sent_at: DateTime<Utc>,
    last_from_me: bool,
    unread_count: i64,
}

async fn list_conversations(
    _flag: ChatEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Conversation>>, AppError> {
    let rows = sqlx::query_as::<_, Conversation>(
        "SELECT f.other AS friend_id,
                COALESCE(NULLIF(btrim(p.display_name), ''), 'Friend') AS display_name,
                left(m.body, $2) AS last_message,
                m.sent_at AS last_sent_at,
                (m.from_user = $1) AS last_from_me,
                (SELECT count(*) FROM messages u
                  WHERE u.from_user = f.other AND u.to_user = $1 AND u.read_at IS NULL)
                    AS unread_count
           FROM (SELECT CASE WHEN user_a = $1 THEN user_b ELSE user_a END AS other
                   FROM friendships WHERE user_a = $1 OR user_b = $1) f
           JOIN LATERAL (
                SELECT body, sent_at, from_user FROM messages
                 WHERE (from_user = $1 AND to_user = f.other)
                    OR (from_user = f.other AND to_user = $1)
                 ORDER BY sent_at DESC, id DESC LIMIT 1) m ON true
           LEFT JOIN profiles p ON p.user_id = f.other
          ORDER BY m.sent_at DESC",
    )
    .bind(me)
    .bind(PREVIEW_CHARS)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

// ---------- thread ----------

#[derive(Serialize, sqlx::FromRow)]
pub struct Message {
    id: Uuid,
    from_user: Uuid,
    to_user: Uuid,
    mine: bool,
    body: String,
    sent_at: DateTime<Utc>,
    read_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct ThreadQuery {
    before: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

async fn get_thread(
    _flag: ChatEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(friend_id): Path<Uuid>,
    Query(q): Query<ThreadQuery>,
) -> Result<Json<Vec<Message>>, AppError> {
    require_friend(&state.pool, me, friend_id).await?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    sqlx::query(
        "UPDATE messages SET read_at = now()
          WHERE from_user = $2 AND to_user = $1 AND read_at IS NULL",
    )
    .bind(me)
    .bind(friend_id)
    .execute(&state.pool)
    .await?;
    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, from_user, to_user, (from_user = $1) AS mine, body, sent_at, read_at
           FROM messages
          WHERE ((from_user = $1 AND to_user = $2) OR (from_user = $2 AND to_user = $1))
            AND ($3::timestamptz IS NULL OR sent_at < $3)
          ORDER BY sent_at DESC, id DESC
          LIMIT $4",
    )
    .bind(me)
    .bind(friend_id)
    .bind(q.before)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

// ---------- send / delete ----------

#[derive(Deserialize, Validate)]
struct SendRequest {
    body: String,
}

async fn send_message(
    _flag: ChatEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path(friend_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<SendRequest>,
) -> Result<(StatusCode, Json<Message>), AppError> {
    require_friend(&state.pool, me, friend_id).await?;
    let body = clean_body(&req.body)?;
    let mut tx = state.pool.begin().await?;
    // Serialise per sender so the rate limit cannot be raced.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 1))")
        .bind(me)
        .execute(&mut *tx)
        .await?;
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages
          WHERE from_user = $1 AND sent_at > now() - interval '1 minute'",
    )
    .bind(me)
    .fetch_one(&mut *tx)
    .await?;
    if recent >= MAX_PER_MINUTE {
        return Err(AppError::BadRequest(format!(
            "Too many messages: limit is {MAX_PER_MINUTE} per minute. Try again shortly."
        )));
    }
    let msg = sqlx::query_as::<_, Message>(
        "INSERT INTO messages (from_user, to_user, body) VALUES ($1, $2, $3)
         RETURNING id, from_user, to_user, true AS mine, body, sent_at, read_at",
    )
    .bind(me)
    .bind(friend_id)
    .bind(&body)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(msg)))
}

async fn delete_message(
    _flag: ChatEnabled,
    AuthUser(me): AuthUser,
    State(state): State<AppState>,
    Path((friend_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_friend(&state.pool, me, friend_id).await?;
    let n = sqlx::query("DELETE FROM messages WHERE id = $1 AND from_user = $2 AND to_user = $3")
        .bind(id)
        .bind(me)
        .bind(friend_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if n == 0 {
        // Unknown message, or not the sender: indistinguishable.
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::clean_body;

    #[test]
    fn body_rules() {
        assert_eq!(clean_body("  hi \n").unwrap(), "hi");
        assert!(clean_body("   ").is_err());
        assert!(clean_body(&"a".repeat(2001)).is_err());
        assert!(clean_body(&"a".repeat(2000)).is_ok());
        assert!(clean_body("a\u{0007}b").is_err());
        assert!(clean_body("a\nb").is_ok());
    }
}

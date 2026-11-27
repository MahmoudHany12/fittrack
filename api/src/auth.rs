//! Email + password auth.
//!
//! - Passwords: Argon2id (PHC string), hashed on the blocking pool.
//! - Access token: short-lived HS256 JWT (`sub` = user id).
//! - Refresh token: opaque 256-bit random value, stored only as a SHA-256 hash,
//!   rotated on every use. Presenting an already-used token revokes every
//!   refresh token of that user (reuse detection).

use crate::{error::AppError, extract::ValidatedJson, AppState};
use anyhow::anyhow;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::OnceLock;
use uuid::Uuid;
use validator::Validate;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
}

// ---------- password hashing ----------

pub async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow!("argon2 hash failed: {e}"))
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .map_err(AppError::Internal)
}

async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || match PasswordHash::new(&hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}

/// A valid hash we verify against when the email is unknown, so login timing
/// does not reveal which emails are registered.
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

async fn dummy_hash() -> String {
    if let Some(h) = DUMMY_HASH.get() {
        return h.clone();
    }
    let h = hash_password("dummy-password-for-timing".into())
        .await
        .unwrap_or_default();
    DUMMY_HASH.get_or_init(|| h).clone()
}

// ---------- tokens ----------

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    iat: i64,
    exp: i64,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

fn sign_access(cfg: &crate::config::Config, user_id: Uuid) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        iat: now.timestamp(),
        exp: (now + cfg.access_ttl).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_access_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(e.into()))
}

fn hash_token(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

async fn issue_tokens(
    conn: &mut PgConnection,
    cfg: &crate::config::Config,
    user_id: Uuid,
) -> Result<TokenResponse, AppError> {
    let mut raw = [0u8; 32];
    OsRng.fill_bytes(&mut raw);
    let refresh_token = URL_SAFE_NO_PAD.encode(raw);
    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(hash_token(&refresh_token))
        .bind(Utc::now() + cfg.refresh_ttl)
        .execute(&mut *conn)
        .await?;
    Ok(TokenResponse {
        access_token: sign_access(cfg, user_id)?,
        refresh_token,
        token_type: "Bearer",
        expires_in: cfg.access_ttl.num_seconds(),
    })
}

// ---------- extractor ----------

/// Authenticated user id, taken from a valid `Authorization: Bearer <jwt>`.
pub struct AuthUser(pub Uuid);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;
        let mut validation = Validation::default(); // HS256, validates exp
        validation.leeway = 5;
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_access_secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::Unauthorized)?;
        Ok(AuthUser(data.claims.sub))
    }
}

// ---------- handlers ----------

#[derive(Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(email, length(max = 254))]
    pub email: String,
    #[validate(length(min = 10, max = 128))]
    pub password: String,
}

#[derive(Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email, length(max = 254))]
    pub email: String,
    #[validate(length(min = 1, max = 128))]
    pub password: String,
}

#[derive(Deserialize, Validate)]
pub struct RefreshRequest {
    #[validate(length(min = 1, max = 256))]
    pub refresh_token: String,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    password_hash: String,
}

type RefreshRow = (Uuid, Uuid, DateTime<Utc>, Option<DateTime<Utc>>);

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

async fn register(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> Result<(StatusCode, Json<TokenResponse>), AppError> {
    let email = normalize_email(&req.email);
    let hash = hash_password(req.password).await?;
    let mut tx = state.pool.begin().await?;
    let inserted: Result<(Uuid,), sqlx::Error> =
        sqlx::query_as("INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id")
            .bind(&email)
            .bind(&hash)
            .fetch_one(&mut *tx)
            .await;
    let user_id = match inserted {
        Ok((id,)) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::Conflict("Email is already registered"));
        }
        Err(e) => return Err(e.into()),
    };
    let tokens = issue_tokens(&mut tx, &state.config, user_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(tokens)))
}

async fn login(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let email = normalize_email(&req.email);
    let user = sqlx::query_as::<_, UserRow>("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    let hash = match &user {
        Some(u) => u.password_hash.clone(),
        None => dummy_hash().await,
    };
    let ok = verify_password(req.password, hash).await;
    let user = match (ok, user) {
        (true, Some(u)) => u,
        _ => return Err(AppError::Unauthorized),
    };
    let mut conn = state.pool.acquire().await?;
    Ok(Json(issue_tokens(&mut conn, &state.config, user.id).await?))
}

async fn refresh(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let row: Option<RefreshRow> = sqlx::query_as(
        "SELECT id, user_id, expires_at, revoked_at FROM refresh_tokens \
         WHERE token_hash = $1 FOR UPDATE",
    )
    .bind(hash_token(&req.refresh_token))
    .fetch_optional(&mut *tx)
    .await?;
    let (id, user_id, expires_at, revoked_at) = row.ok_or(AppError::Unauthorized)?;

    if revoked_at.is_some() {
        // Reuse of a rotated token: assume theft, kill every session.
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Err(AppError::Unauthorized);
    }
    if expires_at < Utc::now() {
        return Err(AppError::Unauthorized);
    }
    sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let tokens = issue_tokens(&mut tx, &state.config, user_id).await?;
    tx.commit().await?;
    Ok(Json(tokens))
}

async fn logout(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RefreshRequest>,
) -> Result<StatusCode, AppError> {
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(hash_token(&req.refresh_token))
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Me {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<Me>, AppError> {
    sqlx::query_as::<_, Me>("SELECT id, email, created_at FROM users WHERE id = $1")
        .bind(user.0)
        .fetch_optional(&state.pool)
        .await?
        .map(Json)
        .ok_or(AppError::Unauthorized)
}

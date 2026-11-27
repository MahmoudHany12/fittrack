use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use fittrack_api::{app, config::Config, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn router(pool: PgPool) -> Router {
    app(AppState {
        pool,
        config: Arc::new(Config::for_test()),
    })
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    call(app, Method::POST, uri, Some(body), None).await
}

async fn register(app: &Router, email: &str) -> Value {
    let (status, body) = post(
        app,
        "/auth/register",
        json!({ "email": email, "password": "correct-horse-battery" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

#[sqlx::test]
async fn register_returns_tokens_and_me_works(pool: PgPool) {
    let app = router(pool);
    let tokens = register(&app, "ann@example.com").await;
    let access = tokens["access_token"].as_str().unwrap();
    let (status, me) = call(&app, Method::GET, "/me", None, Some(access)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "ann@example.com");
    assert!(me.get("password_hash").is_none());
}

#[sqlx::test]
async fn duplicate_email_is_conflict_case_insensitive(pool: PgPool) {
    let app = router(pool);
    register(&app, "ann@example.com").await;
    let (status, body) = post(
        &app,
        "/auth/register",
        json!({ "email": "ANN@Example.com", "password": "correct-horse-battery" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
}

#[sqlx::test]
async fn register_validates_input(pool: PgPool) {
    let app = router(pool);
    let (s, b) = post(
        &app,
        "/auth/register",
        json!({ "email": "not-an-email", "password": "short" }),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["error"]["fields"]["email"].is_array());
    assert!(b["error"]["fields"]["password"].is_array());

    let (s, _) = post(&app, "/auth/register", json!({ "email": "a@b.co" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST); // missing field -> bad_request
}

#[sqlx::test]
async fn login_success_and_failures(pool: PgPool) {
    let app = router(pool);
    register(&app, "ann@example.com").await;

    let (s, b) = post(
        &app,
        "/auth/login",
        json!({ "email": "Ann@example.com", "password": "correct-horse-battery" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["access_token"].is_string());

    let (s, _) = post(
        &app,
        "/auth/login",
        json!({ "email": "ann@example.com", "password": "wrong-password-here" }),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (s, _) = post(
        &app,
        "/auth/login",
        json!({ "email": "nobody@example.com", "password": "whatever-password" }),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn me_requires_valid_token(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/me", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, Method::GET, "/me", None, Some("garbage")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refresh_rotates_and_reuse_revokes_everything(pool: PgPool) {
    let app = router(pool);
    let first = register(&app, "ann@example.com").await;
    let r1 = first["refresh_token"].as_str().unwrap().to_string();

    let (s, second) = post(&app, "/auth/refresh", json!({ "refresh_token": r1 })).await;
    assert_eq!(s, StatusCode::OK);
    let r2 = second["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(r1, r2);

    // Replaying the old token fails and burns the new one too.
    let (s, _) = post(&app, "/auth/refresh", json!({ "refresh_token": r1 })).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(&app, "/auth/refresh", json!({ "refresh_token": r2 })).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refresh_rejects_unknown_token(pool: PgPool) {
    let app = router(pool);
    let (s, _) = post(&app, "/auth/refresh", json!({ "refresh_token": "nope" })).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn logout_revokes_refresh_token(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let r = t["refresh_token"].as_str().unwrap().to_string();
    let (s, _) = post(&app, "/auth/logout", json!({ "refresh_token": r })).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = post(&app, "/auth/refresh", json!({ "refresh_token": r })).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn malformed_json_is_bad_request(pool: PgPool) {
    let app = router(pool);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_client_error());
}

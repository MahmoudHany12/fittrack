//! Leaderboard: opt-in gating, friends-only visibility, ranking, privacy.
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
use uuid::Uuid;

fn router_with(pool: PgPool, config: Config) -> Router {
    app(AppState {
        pool,
        config: Arc::new(config),
    })
}

fn router(pool: PgPool) -> Router {
    router_with(pool, Config::for_test())
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

struct User {
    id: String,
    token: String,
}

async fn register(app: &Router, email: &str, name: &str) -> User {
    let (status, body) = call(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({ "email": email, "password": "correct-horse-battery" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = body["access_token"].as_str().unwrap().to_string();
    let (_, me) = call(app, Method::GET, "/me", None, Some(&token)).await;
    let (s, b) = call(
        app,
        Method::PUT,
        "/profile",
        Some(json!({ "display_name": name, "calorie_target": 2200, "units": "metric" })),
        Some(&token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    User {
        id: me["id"].as_str().unwrap().to_string(),
        token,
    }
}

async fn befriend(app: &Router, a: &User, b: &User) {
    let (s, inv) = call(app, Method::POST, "/friends/invites", None, Some(&a.token)).await;
    assert_eq!(s, StatusCode::CREATED, "{inv}");
    let code = inv["code"].as_str().unwrap();
    let (s, body) = call(
        app,
        Method::POST,
        &format!("/friends/invites/{code}/accept"),
        None,
        Some(&b.token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

async fn two_friends(app: &Router) -> (User, User) {
    let ann = register(app, "ann@example.com", "Ann").await;
    let bob = register(app, "bob@example.com", "Bob").await;
    befriend(app, &ann, &bob).await;
    (ann, bob)
}

async fn optin(app: &Router, u: &User, on: bool) {
    let (s, b) = call(
        app,
        Method::PUT,
        "/leaderboard/optin",
        Some(json!({ "opted_in": on })),
        Some(&u.token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["opted_in"], on);
}

async fn board(app: &Router, u: &User) -> (StatusCode, Value) {
    call(app, Method::GET, "/leaderboard", None, Some(&u.token)).await
}

fn names(b: &Value) -> Vec<String> {
    b.as_array()
        .unwrap()
        .iter()
        .map(|e| e["display_name"].as_str().unwrap().to_string())
        .collect()
}

async fn seed_workout(pool: &PgPool, user: &User, days_ago: i32, sets: i32) {
    let uid: Uuid = user.id.parse().unwrap();
    let wid: Uuid = sqlx::query_scalar(
        "INSERT INTO workouts (user_id, started_at, duration_s) \
         VALUES ($1, now() - $2::int * interval '1 day', 3600) RETURNING id",
    )
    .bind(uid)
    .bind(days_ago)
    .fetch_one(pool)
    .await
    .unwrap();
    for i in 0..sets {
        sqlx::query(
            "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position) \
             SELECT $1, id, 8, 50, $2 FROM exercises \
             WHERE muscle_group NOT IN ('cardio', 'focus') AND owner_id IS NULL LIMIT 1",
        )
        .bind(wid)
        .bind(i)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test]
async fn default_not_opted_in_is_forbidden(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let (s, b) = call(
        &app,
        Method::GET,
        "/leaderboard/optin",
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["opted_in"], false);
    assert_eq!(board(&app, &ann).await.0, StatusCode::FORBIDDEN);
    optin(&app, &ann, true).await;
    let (s, b) = board(&app, &ann).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(names(&b), ["Ann"]);
    assert_eq!(b[0]["is_me"], true);
}

#[sqlx::test]
async fn shows_only_opted_in_friends(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let bob = register(&app, "bob@example.com", "Bob").await;
    let cy = register(&app, "cy@example.com", "Cy").await;
    befriend(&app, &ann, &bob).await;
    befriend(&app, &ann, &cy).await;
    optin(&app, &ann, true).await;
    optin(&app, &bob, true).await;
    let (_, b) = board(&app, &ann).await;
    assert_eq!(names(&b), ["Ann", "Bob"]);
}

#[sqlx::test]
async fn non_friend_never_appears(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let eve = register(&app, "eve@example.com", "Eve").await;
    optin(&app, &ann, true).await;
    optin(&app, &eve, true).await;
    assert_eq!(names(&board(&app, &ann).await.1), ["Ann"]);
    assert_eq!(names(&board(&app, &eve).await.1), ["Eve"]);
}

#[sqlx::test]
async fn opt_out_disappears_immediately(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    optin(&app, &ann, true).await;
    optin(&app, &bob, true).await;
    assert_eq!(names(&board(&app, &ann).await.1), ["Ann", "Bob"]);
    optin(&app, &bob, false).await;
    assert_eq!(names(&board(&app, &ann).await.1), ["Ann"]);
    assert_eq!(board(&app, &bob).await.0, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn unfriend_removes_both_ways(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    optin(&app, &ann, true).await;
    optin(&app, &bob, true).await;
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/friends/{}", bob.id),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(names(&board(&app, &ann).await.1), ["Ann"]);
    assert_eq!(names(&board(&app, &bob).await.1), ["Bob"]);
}

#[sqlx::test]
async fn ranking_order_and_no_stats_last(pool: PgPool) {
    let app = router(pool.clone());
    let ann = register(&app, "ann@example.com", "Ann").await;
    let bob = register(&app, "bob@example.com", "Bob").await;
    let cy = register(&app, "cy@example.com", "Cy").await;
    befriend(&app, &ann, &bob).await;
    befriend(&app, &ann, &cy).await;
    for u in [&ann, &bob, &cy] {
        optin(&app, u, true).await;
    }
    // Cy trained every other day (five on-time sessions), Bob once.
    for d in [9, 7, 5, 3, 1] {
        seed_workout(&pool, &cy, d, 3).await;
    }
    seed_workout(&pool, &bob, 1, 2).await;
    let (_, b) = board(&app, &ann).await;
    let rows = b.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    // Cy trained most; Ann logged nothing, so her stats are null and she is last.
    assert_eq!(names(&b), ["Cy", "Bob", "Ann"], "{b}");
    assert!(rows[0]["total"].as_u64().unwrap() > rows[1]["total"].as_u64().unwrap());
    assert!(rows[2]["total"].is_null());
    assert!(rows[2]["strength"].is_null());
    assert_eq!(rows[0]["rank"], 1);
    assert_eq!(rows[2]["is_me"], true);
}

#[sqlx::test]
async fn cross_user_isolation(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    let cy = register(&app, "cy@example.com", "Cy").await;
    let dan = register(&app, "dan@example.com", "Dan").await;
    befriend(&app, &cy, &dan).await;
    for u in [&ann, &bob, &cy, &dan] {
        optin(&app, u, true).await;
    }
    assert_eq!(names(&board(&app, &ann).await.1), ["Ann", "Bob"]);
    assert_eq!(names(&board(&app, &cy).await.1), ["Cy", "Dan"]);
    // Someone else's opt-out does not affect my setting.
    optin(&app, &cy, false).await;
    let (_, me) = call(
        &app,
        Method::GET,
        "/leaderboard/optin",
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(me["opted_in"], true);
}

#[sqlx::test]
async fn flag_off_is_404_even_unauthenticated(pool: PgPool) {
    let mut config = Config::for_test();
    config.features.remove("leaderboard");
    let app = router_with(pool, config);
    for (m, uri) in [
        (Method::GET, "/leaderboard"),
        (Method::GET, "/leaderboard/optin"),
        (Method::PUT, "/leaderboard/optin"),
    ] {
        let (s, _) = call(&app, m, uri, Some(json!({ "opted_in": true })), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test]
async fn never_exposes_email(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    optin(&app, &ann, true).await;
    optin(&app, &bob, true).await;
    let (_, b) = board(&app, &ann).await;
    let text = b.to_string();
    assert!(
        !text.contains("example.com") && !text.contains("email"),
        "{text}"
    );
    for e in b.as_array().unwrap() {
        let mut keys: Vec<_> = e.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "cardio",
                "display_name",
                "focus",
                "is_me",
                "rank",
                "strength",
                "total",
                "user_id"
            ]
        );
    }
}

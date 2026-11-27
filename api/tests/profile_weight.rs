use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use fittrack_api::{
    app,
    config::Config,
    weight::{weight_trend, Trend},
    AppState,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

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

/// Registers a user; returns (access token, user id).
async fn register(app: &Router, email: &str) -> (String, Uuid) {
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
    (token, me["id"].as_str().unwrap().parse().unwrap())
}

async fn insert_weight(pool: &PgPool, user: Uuid, at: DateTime<Utc>, kg: f64) {
    sqlx::query(
        "INSERT INTO weight_logs (user_id, measured_at, kg) VALUES ($1, $2, $3::float8::numeric(5,2))",
    )
    .bind(user)
    .bind(at)
    .bind(kg)
    .execute(pool)
    .await
    .unwrap();
}

// ---------- profile ----------

#[sqlx::test]
async fn profile_requires_auth(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/profile", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn profile_defaults_then_update(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;

    let (s, p) = call(&app, Method::GET, "/profile", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["calorie_target"], 2200);
    assert_eq!(p["units"], "metric");
    assert!(p["display_name"].is_null());
    assert!(p["weight_goal_kg"].is_null());

    let (s, p) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(json!({
            "display_name": "  Ann  ",
            "calorie_target": 1900,
            "weight_goal_kg": 62.5,
            "units": "imperial"
        })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["display_name"], "Ann");
    assert_eq!(p["weight_goal_kg"], 62.5);

    let (_, p) = call(&app, Method::GET, "/profile", None, Some(&t)).await;
    assert_eq!(p["display_name"], "Ann");
    assert_eq!(p["calorie_target"], 1900);
    assert_eq!(p["units"], "imperial");
    assert!(p["updated_at"].is_string());

    // Second PUT updates the same row and can clear nullable fields.
    let (s, p) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(json!({
            "display_name": null,
            "calorie_target": 2500,
            "weight_goal_kg": null,
            "units": "metric"
        })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(p["display_name"].is_null());
    assert!(p["weight_goal_kg"].is_null());
    assert_eq!(p["calorie_target"], 2500);
}

#[sqlx::test]
async fn profile_validation(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    let (s, b) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(json!({
            "display_name": "x".repeat(51),
            "calorie_target": 100,
            "weight_goal_kg": 5.0,
            "units": "stones"
        })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    for f in ["display_name", "calorie_target", "weight_goal_kg", "units"] {
        assert!(b["error"]["fields"][f].is_array(), "missing error for {f}");
    }
    let (s, _) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(json!({ "calorie_target": 10001, "units": "metric" })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Nothing was stored.
    let (_, p) = call(&app, Method::GET, "/profile", None, Some(&t)).await;
    assert!(p["updated_at"].is_null());
}

#[sqlx::test]
async fn profiles_are_per_user(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;
    call(
        &app,
        Method::PUT,
        "/profile",
        Some(json!({ "display_name": "Ann", "calorie_target": 1800, "units": "metric" })),
        Some(&a),
    )
    .await;
    let (_, p) = call(&app, Method::GET, "/profile", None, Some(&b)).await;
    assert!(p["display_name"].is_null());
    assert_eq!(p["calorie_target"], 2200);
}

// ---------- weight CRUD ----------

#[sqlx::test]
async fn weight_crud(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    let now = Utc::now();
    let older = (now - Duration::days(2)).to_rfc3339();
    let newer = (now - Duration::hours(1)).to_rfc3339();

    let (s, a) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": 80.55, "measured_at": older })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    assert_eq!(a["kg"], 80.55);
    let (s, b) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": 79.9, "measured_at": newer })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // Newest first.
    let (s, list) = call(&app, Method::GET, "/weight", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], b["id"]);
    assert_eq!(list[1]["id"], a["id"]);

    // Range filter.
    let from = (now - Duration::days(1)).to_rfc3339().replace('+', "%2B");
    let (_, ranged) = call(
        &app,
        Method::GET,
        &format!("/weight?from={from}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(ranged.as_array().unwrap().len(), 1);

    // Update.
    let id = a["id"].as_str().unwrap();
    let (s, u) = call(
        &app,
        Method::PUT,
        &format!("/weight/{id}"),
        Some(json!({ "kg": 81.0 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert_eq!(u["kg"], 81.0);
    assert_eq!(u["measured_at"], a["measured_at"]);

    // Soft delete.
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/weight/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&t)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/weight/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/weight/{id}"),
        Some(json!({ "kg": 70.0 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn weight_validation_and_auth(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    for kg in [19.9, 400.1, -5.0] {
        let (s, b) = call(
            &app,
            Method::POST,
            "/weight",
            Some(json!({ "kg": kg })),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "kg={kg}");
        assert!(b["error"]["fields"]["kg"].is_array());
    }
    let (s, _) = call(&app, Method::POST, "/weight", Some(json!({})), Some(&t)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, Method::GET, "/weight", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": 80 })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn weight_is_isolated_between_users(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;
    let (_, log) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": 70.0 })),
        Some(&a),
    )
    .await;
    let id = log["id"].as_str().unwrap();

    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&b)).await;
    assert!(list.as_array().unwrap().is_empty());
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/weight/{id}"),
        Some(json!({ "kg": 99.0 })),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/weight/{id}"),
        None,
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Owner's row is untouched.
    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&a)).await;
    assert_eq!(list[0]["kg"], 70.0);
}

#[sqlx::test]
async fn post_with_id_is_idempotent_and_not_hijackable(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;
    let id = Uuid::new_v4();
    let at = (Utc::now() - Duration::hours(3)).to_rfc3339();
    let body = json!({ "id": id, "kg": 75.0, "measured_at": at });

    let (s, first) = call(&app, Method::POST, "/weight", Some(body.clone()), Some(&a)).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(first["id"], id.to_string());
    let (s, again) = call(&app, Method::POST, "/weight", Some(body), Some(&a)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again["id"], first["id"]);
    assert_eq!(again["measured_at"], first["measured_at"]);
    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&a)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Replay with a changed value updates in place.
    let (s, upd) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "id": id, "kg": 74.0 })),
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(upd["kg"], 74.0);
    assert_eq!(upd["measured_at"], first["measured_at"]);

    // Another user can never overwrite it.
    let (s, _) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "id": id, "kg": 50.0 })),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&a)).await;
    assert_eq!(list[0]["kg"], 74.0);
    let (_, list) = call(&app, Method::GET, "/weight", None, Some(&b)).await;
    assert!(list.as_array().unwrap().is_empty());
}

// ---------- trend ----------

async fn trend_for(pool: &PgPool, user: Uuid, now: DateTime<Utc>) -> (Option<f64>, Trend) {
    let t = weight_trend(pool, user, now).await.unwrap();
    (t.current_kg, t.trend)
}

#[sqlx::test]
async fn trend_up_down_flat(pool: PgPool) {
    let app = router(pool.clone());
    let (_, user) = register(&app, "ann@example.com").await;
    let now = Utc::now();

    // previous week: 80.0 and 80.4 -> mean 80.2
    insert_weight(&pool, user, now - Duration::days(12), 80.0).await;
    insert_weight(&pool, user, now - Duration::days(9), 80.4).await;
    // last week: 81.0 -> up
    insert_weight(&pool, user, now - Duration::days(2), 81.0).await;
    assert_eq!(trend_for(&pool, user, now).await, (Some(81.0), Trend::Up));

    // Add a lower reading: mean(81.0, 79.0) = 80.0 vs 80.2 -> flat
    insert_weight(&pool, user, now - Duration::hours(5), 79.0).await;
    assert_eq!(trend_for(&pool, user, now).await, (Some(79.0), Trend::Flat));

    // Another lower reading: mean(81, 79, 78) = 79.33 vs 80.2 -> down
    insert_weight(&pool, user, now - Duration::hours(2), 78.0).await;
    assert_eq!(trend_for(&pool, user, now).await, (Some(78.0), Trend::Down));
}

#[sqlx::test]
async fn trend_insufficient_data_is_flat(pool: PgPool) {
    let app = router(pool.clone());
    let (_, user) = register(&app, "ann@example.com").await;
    let now = Utc::now();

    assert_eq!(trend_for(&pool, user, now).await, (None, Trend::Flat));

    insert_weight(&pool, user, now - Duration::days(1), 90.0).await;
    assert_eq!(trend_for(&pool, user, now).await, (Some(90.0), Trend::Flat));

    // Only old data (outside both windows) is not enough either.
    let (_, other) = register(&app, "bob@example.com").await;
    insert_weight(&pool, other, now - Duration::days(30), 60.0).await;
    insert_weight(&pool, other, now - Duration::days(1), 70.0).await;
    assert_eq!(
        trend_for(&pool, other, now).await,
        (Some(70.0), Trend::Flat)
    );
}

#[sqlx::test]
async fn trend_ignores_deleted_and_other_users(pool: PgPool) {
    let app = router(pool.clone());
    let (token, user) = register(&app, "ann@example.com").await;
    let (_, other) = register(&app, "bob@example.com").await;
    let now = Utc::now();

    insert_weight(&pool, user, now - Duration::days(10), 80.0).await;
    insert_weight(&pool, user, now - Duration::days(1), 80.1).await;
    // Another user's big jump must not leak in.
    insert_weight(&pool, other, now - Duration::days(10), 50.0).await;
    insert_weight(&pool, other, now - Duration::days(1), 99.0).await;
    // A big recent reading that is then soft-deleted via the API.
    let (_, log) = call(
        &app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": 95.0, "measured_at": (now - Duration::hours(1)).to_rfc3339() })),
        Some(&token),
    )
    .await;
    let id = log["id"].as_str().unwrap();
    assert_eq!(trend_for(&pool, user, now).await.1, Trend::Up);
    call(
        &app,
        Method::DELETE,
        &format!("/weight/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(trend_for(&pool, user, now).await, (Some(80.1), Trend::Flat));
}

#[test]
fn trend_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Trend::Up).unwrap(), "\"up\"");
    assert_eq!(serde_json::to_string(&Trend::Flat).unwrap(), "\"flat\"");
    assert_eq!(serde_json::to_string(&Trend::Down).unwrap(), "\"down\"");
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use fittrack_api::{app, config::Config, AppState};
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

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Registers a user, returns (access token, user id).
async fn register(app: &Router, email: &str) -> (String, Uuid) {
    let req = Request::builder()
        .method("POST")
        .uri("/auth/register")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": email, "password": "correct-horse-battery" }).to_string(),
        ))
        .unwrap();
    let (s, body) = send(app, req).await;
    assert_eq!(s, StatusCode::CREATED);
    let token = body["access_token"].as_str().unwrap().to_string();
    let req = Request::builder()
        .uri("/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (_, me) = send(app, req).await;
    (token, me["id"].as_str().unwrap().parse().unwrap())
}

async fn dashboard(app: &Router, token: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri("/dashboard")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    let req = Request::builder()
        .uri("/dashboard")
        .body(Body::empty())
        .unwrap();
    let (s, _) = send(&app, req).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn empty_account_returns_nulls_not_errors(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let (s, d) = dashboard(&app, &token).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["name"], "ann");
    assert_eq!(d["status"]["key"], "no_data");
    assert_eq!(d["status"]["label"], "No data yet");
    assert!(d["readiness"]["score"].is_null());
    assert!(d["sleep"]["minutes"].is_null());
    assert_eq!(d["diet"]["kcal_eaten"], 0);
    assert_eq!(d["diet"]["kcal_target"], 2200);
    assert_eq!(d["exercise"]["workout_count_today"], 0);
    assert!(d["weight"]["kg"].is_null());
    assert_eq!(d["weight"]["trend"], "flat");
}

#[sqlx::test]
async fn aggregates_todays_data_for_the_caller_only(pool: PgPool) {
    let app = router(pool.clone());
    let (token, uid) = register(&app, "ann@example.com").await;
    let (_, other) = register(&app, "bob@example.com").await;

    sqlx::query(
        "INSERT INTO sleep_logs (user_id, bedtime, wake_time, quality) \
         VALUES ($1, now() - interval '9 hours', now() - interval '1 hour', 4)",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workouts (user_id, started_at, duration_s) \
         VALUES ($1, greatest(now() - interval '1 minute', date_trunc('day', now())), 1800)",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO weight_logs (user_id, measured_at, kg) VALUES ($1, now(), 78.4)")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    for owner in [uid, other] {
        let food: (Uuid,) = sqlx::query_as(
            "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
             VALUES ('Rice', 500, 10, 100, 1, 150, 'custom', $1) RETURNING id",
        )
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g) \
             VALUES (gen_random_uuid(), $1, $2, greatest(now() - interval '1 minute', date_trunc('day', now())), 2, 1000, 20, 200, 2)",
        )
        .bind(owner)
        .bind(food.0)
        .execute(&pool)
        .await
        .unwrap();
    }

    let (s, d) = dashboard(&app, &token).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["sleep"]["minutes"], 480);
    assert_eq!(d["sleep"]["quality"], 4);
    assert_eq!(d["diet"]["kcal_eaten"], 1000); // only the caller's log
    assert_eq!(d["exercise"]["workout_count_today"], 1);
    assert_eq!(d["exercise"]["workout_minutes_today"], 30);
    assert_eq!(d["weight"]["kg"], 78.4);
    assert!(d["readiness"]["score"].as_u64().unwrap() > 0);
    assert_ne!(d["status"]["key"], "no_data");
}

fn app_with(pool: PgPool, cfg: Config) -> Router {
    app(AppState {
        pool,
        config: Arc::new(cfg),
    })
}

#[sqlx::test]
async fn stats_included_only_when_flag_on_and_data_exists(pool: PgPool) {
    let app = router(pool.clone());
    let (token, uid) = register(&app, "ann@example.com").await;
    // No activity yet: stats is null even with the flag on.
    let (_, d) = dashboard(&app, &token).await;
    assert!(d["stats"].is_null());

    sqlx::query(
        "INSERT INTO workouts (user_id, started_at, duration_s) VALUES ($1, now() - interval '1 day', 3600)",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    let (_, d) = dashboard(&app, &token).await;
    assert!(d["stats"]["strength"]["level"].as_u64().is_some(), "{d}");

    // Flag off: stats stays null.
    let mut cfg = Config::for_test();
    cfg.features.clear();
    let off = app_with(pool, cfg);
    let (_, d) = dashboard(&off, &token).await;
    assert!(d["stats"].is_null());
}

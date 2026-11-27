use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use fittrack_api::{app, config::Config, stats, AppState};
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

/// Registers a user and returns (access token, user id).
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

/// Inserts a workout `days_ago` days back with `sets` sets of a catalog exercise.
async fn seed_workout(pool: &PgPool, user: Uuid, days_ago: i64, exercise: &str, sets: i32) {
    let wid: Uuid = sqlx::query_scalar(
        "INSERT INTO workouts (user_id, started_at, duration_s) VALUES ($1, $2, 3600) RETURNING id",
    )
    .bind(user)
    .bind(Utc::now() - Duration::days(days_ago) - Duration::minutes(5))
    .fetch_one(pool)
    .await
    .unwrap();
    for i in 0..sets {
        sqlx::query(
            "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position) \
             SELECT $1, id, 8, 50, $2 FROM exercises WHERE name = $3 AND owner_id IS NULL",
        )
        .bind(wid)
        .bind(i)
        .bind(exercise)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn seed_sleep(pool: &PgPool, user: Uuid, days_ago: i64, hours: i64) {
    let wake = Utc::now() - Duration::days(days_ago) - Duration::hours(1);
    sqlx::query("INSERT INTO sleep_logs (user_id, bedtime, wake_time) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(wake - Duration::hours(hours))
        .bind(wake)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/stats", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn flag_off_is_404(pool: PgPool) {
    let mut cfg = Config::for_test();
    cfg.features.clear();
    let app = router_with(pool, cfg);
    let (tok, _) = register(&app, "ann@example.com").await;
    let (s, _) = call(&app, Method::GET, "/stats", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn no_data_returns_null_and_stores_nothing(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    let (s, body) = call(&app, Method::GET, "/stats", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.is_null());
    assert!(stats::current_stats(&pool, uid, Utc::now())
        .await
        .unwrap()
        .is_none());
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM character_stats")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn stats_reflect_activity_and_are_isolated(pool: PgPool) {
    let app = router(pool.clone());
    let (ann, ann_id) = register(&app, "ann@example.com").await;
    let (bob, bob_id) = register(&app, "bob@example.com").await;

    for d in 0..6 {
        seed_workout(&pool, ann_id, d, "Bench Press", 5).await;
        seed_sleep(&pool, ann_id, d, 8).await;
    }
    seed_workout(&pool, ann_id, 1, "Running", 1).await;
    // Bob only ran once, a long time ago relative to Ann.
    seed_workout(&pool, bob_id, 3, "Running", 1).await;

    let (s, a) = call(&app, Method::GET, "/stats", None, Some(&ann)).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let (s, b) = call(&app, Method::GET, "/stats", None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK, "{b}");

    for k in ["strength", "cardio", "focus"] {
        let st = &a[k];
        assert!(st["value"].as_u64().unwrap() <= 100);
        assert!((1..=4).contains(&st["level"].as_u64().unwrap()));
        let p = st["progress"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&p));
    }
    // Ann lifts a lot, Bob never lifted.
    assert!(a["strength"]["value"].as_u64().unwrap() > 40);
    assert_eq!(b["strength"]["value"], 10);
    assert_eq!(b["strength"]["level"], 1);
    assert!(a["strength"]["value"].as_u64() > b["strength"]["value"].as_u64());
    // Bob's cardio comes from his own run.
    assert!(b["cardio"]["value"].as_u64().unwrap() > 10);

    // Snapshots are stored per user, one row per day, upserted on re-read.
    let (_, again) = call(&app, Method::GET, "/stats", None, Some(&ann)).await;
    assert_eq!(again["strength"]["value"], a["strength"]["value"]);
    let rows: Vec<(Uuid, i16)> =
        sqlx::query_as("SELECT user_id, strength FROM character_stats ORDER BY user_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    let ann_row = rows.iter().find(|r| r.0 == ann_id).unwrap();
    assert_eq!(
        i64::from(ann_row.1),
        a["strength"]["value"].as_i64().unwrap()
    );
}

#[sqlx::test]
async fn poor_sleep_and_deleted_workouts(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    seed_workout(&pool, uid, 0, "Squat", 10).await;
    let (_, rested) = call(&app, Method::GET, "/stats", None, Some(&tok)).await;
    for d in 0..7 {
        seed_sleep(&pool, uid, d, 4).await;
    }
    let (_, tired) = call(&app, Method::GET, "/stats", None, Some(&tok)).await;
    assert!(
        tired["strength"]["value"].as_u64().unwrap()
            < rested["strength"]["value"].as_u64().unwrap()
    );

    // Soft-deleted workouts do not count.
    sqlx::query("UPDATE workouts SET deleted_at = now() WHERE user_id = $1")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    let (_, after) = call(&app, Method::GET, "/stats", None, Some(&tok)).await;
    assert_eq!(after["strength"]["value"], 10);
}

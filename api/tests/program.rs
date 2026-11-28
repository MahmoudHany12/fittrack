use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
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

async fn register(app: &Router, email: &str) -> String {
    let (status, body) = call(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({ "email": email, "password": "correct-horse-battery" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["access_token"].as_str().unwrap().to_string()
}

async fn exercise_id(app: &Router, tok: &str, name: &str) -> String {
    let uri = format!("/exercises?q={}", name.replace(' ', "%20"));
    let (_, list) = call(app, Method::GET, &uri, None, Some(tok)).await;
    list.as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no exercise {name}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Logs a workout `days_ago` days back with `sets` x `reps` at `kg`.
async fn session(
    app: &Router,
    tok: &str,
    ex: &str,
    days_ago: i64,
    sets: usize,
    reps: i32,
    kg: f64,
) {
    let one = json!({ "exercise_id": ex, "reps": reps, "weight_kg": kg });
    let (s, b) = call(
        app,
        Method::POST,
        "/workouts",
        Some(json!({
            "started_at": Utc::now() - Duration::days(days_ago),
            "duration_s": 3000,
            "sets": vec![one; sets],
        })),
        Some(tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
}

async fn program(app: &Router, tok: &str) -> Value {
    let (s, p) = call(app, Method::GET, "/program", None, Some(tok)).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    p
}

fn weight_of(p: &Value, name: &str) -> Value {
    p["exercises"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("{name} not in {p}"))["weight_kg"]
        .clone()
}

#[sqlx::test]
async fn new_user_gets_session_a_with_no_weights(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let p = program(&app, &tok).await;
    assert_eq!(p["name"], "Strength basics");
    assert_eq!(p["title"], "Workout A");
    assert_eq!((p["week"].clone(), p["day"].clone()), (json!(1), json!(1)));
    assert_eq!(p["sessions_done"], 0);
    assert_eq!(p["rest"], json!({ "min_s": 60, "max_s": 120 }));
    let ex = p["exercises"].as_array().unwrap();
    let names: Vec<_> = ex.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["Squat", "Kettlebell Swing", "Bench Press", "Pull-Up"]
    );
    for e in ex {
        assert!(e["weight_kg"].is_null());
        assert_eq!(e["reason"], "start");
        assert_eq!(e["note"], "Pick a starting weight");
        assert!(e["exercise_id"].is_string(), "{e}");
        assert_eq!(e["sets"], 3);
    }
    assert_eq!(ex[0]["reps"], 5);
    assert_eq!(ex[1]["reps"], 15);
    assert_eq!(ex[0]["superset"], "a");
    assert_eq!(ex[1]["superset"], "a");
    assert!(ex[2]["superset"].is_null());
}

#[sqlx::test]
async fn weights_follow_the_last_sessions(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let squat = exercise_id(&app, &tok, "Squat").await;
    let dead = exercise_id(&app, &tok, "Deadlift").await;

    // One squat session with every rep done: session B next, squat target +2.5 later.
    session(&app, &tok, &squat, 3, 3, 5, 100.0).await;
    let p = program(&app, &tok).await;
    assert_eq!(p["title"], "Workout B");
    assert_eq!(p["sessions_done"], 1);
    assert_eq!(p["day"], 2);
    assert!(weight_of(&p, "Deadlift").is_null());

    // A deadlift session (+5 kg next time) and we are back to A with a squat target.
    session(&app, &tok, &dead, 2, 3, 5, 140.0).await;
    let p = program(&app, &tok).await;
    assert_eq!(p["title"], "Workout A");
    assert_eq!(weight_of(&p, "Squat"), json!(102.5));
    let squat_row = &p["exercises"][0];
    assert_eq!(squat_row["reason"], "increase");
}

#[sqlx::test]
async fn two_missed_sessions_in_a_row_deload_ten_percent(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let squat = exercise_id(&app, &tok, "Squat").await;
    // Reps of 4 against a target of 5, twice.
    session(&app, &tok, &squat, 5, 3, 4, 100.0).await;
    let p = program(&app, &tok).await;
    // one miss: repeat the weight
    session(&app, &tok, &squat, 3, 3, 4, 100.0).await;
    let p2 = program(&app, &tok).await;
    assert_eq!(p["title"], "Workout B");
    assert_eq!(p2["title"], "Workout A");
    assert_eq!(weight_of(&p2, "Squat"), json!(90.0));
    assert_eq!(p2["exercises"][0]["reason"], "deload");
}

#[sqlx::test]
async fn one_missed_session_repeats_the_weight(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;
    session(&app, &tok, &bench, 6, 3, 5, 60.0).await; // done
    session(&app, &tok, &bench, 3, 3, 3, 62.5).await; // missed reps
    let p = program(&app, &tok).await;
    assert_eq!(weight_of(&p, "Bench Press"), json!(62.5));
    let bench_row = p["exercises"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Bench Press")
        .unwrap();
    assert_eq!(bench_row["reason"], "repeat");
}

#[sqlx::test]
async fn other_users_history_and_custom_exercises_do_not_leak(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let squat = exercise_id(&app, &ann, "Squat").await;
    session(&app, &ann, &squat, 2, 3, 5, 100.0).await;
    let p = program(&app, &bob).await;
    assert_eq!(p["sessions_done"], 0);
    assert!(weight_of(&p, "Squat").is_null());
}

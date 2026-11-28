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

/// Logs a workout `hours_ago` hours back with one set; returns the response.
async fn lift(
    app: &Router,
    tok: &str,
    ex: &str,
    hours_ago: i64,
    reps: i32,
    kg: f64,
) -> (StatusCode, Value) {
    call(
        app,
        Method::POST,
        "/workouts",
        Some(json!({
            "started_at": Utc::now() - Duration::hours(hours_ago),
            "duration_s": 3000,
            "sets": [{ "exercise_id": ex, "reps": reps, "weight_kg": kg }],
        })),
        Some(tok),
    )
    .await
}

#[sqlx::test]
async fn first_lift_is_not_a_pr_then_heavier_and_more_reps_are(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;

    let (s, w) = lift(&app, &tok, &bench, 72, 5, 60.0).await;
    assert_eq!(s, StatusCode::CREATED, "{w}");
    assert_eq!(w["personal_bests"], json!([]));
    // existing response fields are untouched
    assert_eq!(w["sets"].as_array().unwrap().len(), 1);

    // Heavier single: weight PR only (e1rm 69.7 < 70).
    let (_, w) = lift(&app, &tok, &bench, 48, 1, 67.0).await;
    let prs = w["personal_bests"].as_array().unwrap();
    assert_eq!(prs.len(), 1, "{w}");
    assert_eq!(prs[0]["kind"], "weight");
    assert_eq!(prs[0]["value"], 67.0);
    assert_eq!(prs[0]["previous"], 60.0);
    assert_eq!(prs[0]["exercise_name"], "Bench Press");

    // Same weight as before: not a PR.
    let (_, w) = lift(&app, &tok, &bench, 24, 1, 67.0).await;
    assert_eq!(w["personal_bests"], json!([]), "{w}");

    // More volume at a lower weight: e1rm PR only. 60 x 10 -> 80.
    let (_, w) = lift(&app, &tok, &bench, 2, 10, 60.0).await;
    let prs = w["personal_bests"].as_array().unwrap();
    assert_eq!(prs.len(), 1, "{w}");
    assert_eq!(prs[0]["kind"], "e1rm");
    assert_eq!(prs[0]["value"], 80.0);
}

#[sqlx::test]
async fn records_list_best_weight_and_e1rm_per_exercise(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;
    let squat = exercise_id(&app, &tok, "Squat").await;
    lift(&app, &tok, &bench, 72, 5, 60.0).await;
    lift(&app, &tok, &bench, 48, 1, 67.0).await;
    lift(&app, &tok, &squat, 24, 5, 100.0).await;

    let (s, r) = call(&app, Method::GET, "/records", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let r = r.as_array().unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0]["exercise_name"], "Bench Press");
    assert_eq!(r[0]["best_weight_kg"], 67.0);
    assert_eq!(r[0]["best_e1rm_kg"], 70.0); // 60 x 5
    assert_eq!(r[0]["best_e1rm_weight_kg"], 60.0);
    assert_eq!(r[0]["best_e1rm_reps"], 5);
    assert_eq!(r[1]["exercise_name"], "Squat");
    assert_eq!(r[1]["best_e1rm_kg"], 116.67);
}

#[sqlx::test]
async fn editing_a_workout_reports_its_prs_and_deleted_ones_do_not_count(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;
    lift(&app, &tok, &bench, 72, 5, 60.0).await;
    let (_, second) = lift(&app, &tok, &bench, 24, 5, 60.0).await;
    assert_eq!(second["personal_bests"], json!([]));
    let id = second["id"].as_str().unwrap();

    // PUT with heavier sets: now a PR.
    let (s, w) = call(
        &app,
        Method::PUT,
        &format!("/workouts/{id}"),
        Some(json!({
            "duration_s": 3000,
            "sets": [{ "exercise_id": bench, "reps": 5, "weight_kg": 70.0 }],
        })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    assert_eq!(w["personal_bests"].as_array().unwrap().len(), 2);

    // After deleting it, /records is back to the first workout.
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/workouts/{id}"),
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, r) = call(&app, Method::GET, "/records", None, Some(&tok)).await;
    assert_eq!(r[0]["best_weight_kg"], 60.0);
}

#[sqlx::test]
async fn progression_lists_recent_prs_for_24_hours(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;
    lift(&app, &tok, &bench, 100, 5, 60.0).await;
    lift(&app, &tok, &bench, 30, 5, 65.0).await; // PR, but 30 h ago
    let (_, p) = call(&app, Method::GET, "/progression", None, Some(&tok)).await;
    assert_eq!(p["new_personal_bests"], json!([]), "{p}");
    lift(&app, &tok, &bench, 1, 5, 70.0).await;
    let (_, p) = call(&app, Method::GET, "/progression", None, Some(&tok)).await;
    let prs = p["new_personal_bests"].as_array().unwrap();
    assert_eq!(prs.len(), 2, "{p}");
    assert!(prs.iter().all(|x| x["previous"].as_f64().unwrap() > 0.0));
}

#[sqlx::test]
async fn records_are_isolated_per_user(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let bench = exercise_id(&app, &ann, "Bench Press").await;
    lift(&app, &ann, &bench, 72, 5, 200.0).await;
    // Bob's first lifts are not PRs against Ann's history.
    lift(&app, &bob, &bench, 48, 5, 50.0).await;
    let (_, w) = lift(&app, &bob, &bench, 24, 5, 55.0).await;
    let prs = w["personal_bests"].as_array().unwrap();
    assert_eq!(prs[0]["previous"], 50.0, "{w}");
    let (_, r) = call(&app, Method::GET, "/records", None, Some(&bob)).await;
    assert_eq!(r[0]["best_weight_kg"], 55.0);
    let (_, none) = call(
        &app,
        Method::GET,
        "/records",
        None,
        Some(&register(&app, "cy@example.com").await),
    )
    .await;
    assert_eq!(none, json!([]));
}

#[sqlx::test]
async fn flag_off_workouts_report_no_personal_bests(pool: PgPool) {
    let mut cfg = Config::for_test();
    cfg.features.clear();
    let app = router_with(pool, cfg);
    let tok = register(&app, "ann@example.com").await;
    let bench = exercise_id(&app, &tok, "Bench Press").await;
    lift(&app, &tok, &bench, 72, 5, 60.0).await;
    let (_, w) = lift(&app, &tok, &bench, 24, 5, 80.0).await;
    assert_eq!(w["personal_bests"], json!([]));
}

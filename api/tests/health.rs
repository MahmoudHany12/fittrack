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

async fn consent(app: &Router, tok: &str, steps: bool, workouts: bool, sleep: bool) {
    let (s, b) = call(
        app,
        Method::PUT,
        "/health/consents",
        Some(json!({ "steps": steps, "workouts": workouts, "sleep": sleep })),
        Some(tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

fn payload(prefix: &str) -> Value {
    let now = Utc::now();
    json!({
        "steps": [
            { "external_id": format!("{prefix}-s1"), "date": now.date_naive().to_string(), "count": 5000 },
            { "external_id": format!("{prefix}-s2"), "date": (now - Duration::days(1)).date_naive().to_string(), "count": 8000 },
        ],
        "workouts": [
            { "external_id": format!("{prefix}-w1"), "started_at": (now - Duration::hours(5)).to_rfc3339(), "duration_s": 1800, "kind": "running" },
        ],
        "sleep": [
            { "external_id": format!("{prefix}-z1"), "bedtime": (now - Duration::hours(16)).to_rfc3339(), "wake_time": (now - Duration::hours(8)).to_rfc3339() },
        ],
    })
}

#[sqlx::test]
async fn consent_defaults_off_and_roundtrips(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "a@example.com").await;
    let (s, b) = call(&app, Method::GET, "/health/consents", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        b,
        json!({ "steps": false, "workouts": false, "sleep": false })
    );
    consent(&app, &tok, true, false, true).await;
    let (_, b) = call(&app, Method::GET, "/health/consents", None, Some(&tok)).await;
    assert_eq!(
        b,
        json!({ "steps": true, "workouts": false, "sleep": true })
    );
    let (s, _) = call(&app, Method::GET, "/health/consents", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn import_respects_consent(pool: PgPool) {
    let app = router(pool.clone());
    let tok = register(&app, "a@example.com").await;
    // No consent at all: everything skipped, nothing stored.
    let (s, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("x")),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        b["steps"],
        json!({ "imported": 0, "skipped": 2, "duplicates": 0 })
    );
    assert_eq!(b["workouts"]["skipped"], 1);
    assert_eq!(b["sleep"]["skipped"], 1);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM health_samples")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // Only sleep consented.
    consent(&app, &tok, false, false, true).await;
    let (_, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("x")),
        Some(&tok),
    )
    .await;
    assert_eq!(
        b["sleep"],
        json!({ "imported": 1, "skipped": 0, "duplicates": 0 })
    );
    assert_eq!(b["steps"]["skipped"], 2);
    assert_eq!(b["workouts"]["skipped"], 1);

    // Revoking keeps data but stops imports.
    consent(&app, &tok, false, false, false).await;
    let (_, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("y")),
        Some(&tok),
    )
    .await;
    assert_eq!(b["sleep"]["skipped"], 1);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM sleep_logs WHERE source = 'health'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn import_is_idempotent(pool: PgPool) {
    let app = router(pool.clone());
    let tok = register(&app, "a@example.com").await;
    consent(&app, &tok, true, true, true).await;
    let (_, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("x")),
        Some(&tok),
    )
    .await;
    assert_eq!(
        b["steps"],
        json!({ "imported": 2, "skipped": 0, "duplicates": 0 })
    );
    assert_eq!(b["workouts"]["imported"], 1);
    assert_eq!(b["sleep"]["imported"], 1);
    let (_, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("x")),
        Some(&tok),
    )
    .await;
    assert_eq!(
        b["steps"],
        json!({ "imported": 0, "skipped": 0, "duplicates": 2 })
    );
    assert_eq!(b["workouts"]["duplicates"], 1);
    assert_eq!(b["sleep"]["duplicates"], 1);
    for (t, expected) in [("health_samples", 2), ("workouts", 1), ("sleep_logs", 1)] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {t}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, expected, "{t}");
    }
    let src: String = sqlx::query_scalar("SELECT source FROM workouts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(src, "health");
}

#[sqlx::test]
async fn duplicates_within_one_request_and_changed_steps(pool: PgPool) {
    let app = router(pool.clone());
    let tok = register(&app, "a@example.com").await;
    consent(&app, &tok, true, true, true).await;
    let mut p = payload("x");
    let w = p["workouts"][0].clone();
    p["workouts"].as_array_mut().unwrap().push(w);
    let (_, b) = call(&app, Method::POST, "/health/import", Some(p), Some(&tok)).await;
    assert_eq!(
        b["workouts"],
        json!({ "imported": 1, "skipped": 0, "duplicates": 1 })
    );

    // Same step id with a larger count updates the sample.
    let mut p = payload("x");
    p["steps"][0]["count"] = json!(6000);
    let (_, b) = call(&app, Method::POST, "/health/import", Some(p), Some(&tok)).await;
    assert_eq!(
        b["steps"],
        json!({ "imported": 1, "skipped": 0, "duplicates": 1 })
    );
    let v: i64 =
        sqlx::query_scalar("SELECT value::bigint FROM health_samples WHERE external_id = 'x-s1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v, 6000);
}

#[sqlx::test]
async fn validation_errors(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "a@example.com").await;
    consent(&app, &tok, true, true, true).await;
    let now = Utc::now();
    let cases = vec![
        json!({ "steps": [{ "external_id": "", "date": "2024-01-01", "count": 1 }] }),
        json!({ "steps": [{ "external_id": "a", "date": "2024-01-01", "count": -5 }] }),
        json!({ "steps": [{ "external_id": "a", "date": "2024-01-01", "count": 99999999 }] }),
        json!({ "steps": [{ "external_id": "a", "date": (now + Duration::days(30)).date_naive().to_string(), "count": 1 }] }),
        json!({ "workouts": [{ "external_id": "a", "started_at": now.to_rfc3339(), "duration_s": 0, "kind": "run" }] }),
        json!({ "workouts": [{ "external_id": "a", "started_at": now.to_rfc3339(), "duration_s": 90000, "kind": "run" }] }),
        json!({ "workouts": [{ "external_id": "a", "started_at": now.to_rfc3339(), "duration_s": 60, "kind": "  " }] }),
        json!({ "workouts": [{ "external_id": "a", "started_at": (now + Duration::days(30)).to_rfc3339(), "duration_s": 60, "kind": "run" }] }),
        json!({ "sleep": [{ "external_id": "a", "bedtime": now.to_rfc3339(), "wake_time": (now - Duration::hours(1)).to_rfc3339() }] }),
        json!({ "sleep": [{ "external_id": "a", "bedtime": (now - Duration::hours(30)).to_rfc3339(), "wake_time": now.to_rfc3339() }] }),
        json!({ "sleep": [{ "external_id": "a".repeat(201), "bedtime": (now - Duration::hours(8)).to_rfc3339(), "wake_time": now.to_rfc3339() }] }),
    ];
    for c in cases {
        let (s, b) = call(
            &app,
            Method::POST,
            "/health/import",
            Some(c.clone()),
            Some(&tok),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{c} -> {b}");
    }
    // Malformed shape is rejected by the JSON extractor (4xx).
    let (s, _) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(json!({ "steps": [{ "external_id": "a" }] })),
        Some(&tok),
    )
    .await;
    assert!(s.is_client_error());
}

#[sqlx::test]
async fn one_bad_item_stores_nothing(pool: PgPool) {
    let app = router(pool.clone());
    let tok = register(&app, "a@example.com").await;
    consent(&app, &tok, true, true, true).await;
    let mut p = payload("x");
    p["sleep"][0]["wake_time"] = p["sleep"][0]["bedtime"].clone();
    let (s, _) = call(&app, Method::POST, "/health/import", Some(p), Some(&tok)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM health_samples")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn cross_user_isolation(pool: PgPool) {
    let app = router(pool.clone());
    let a = register(&app, "a@example.com").await;
    let b = register(&app, "b@example.com").await;
    consent(&app, &a, true, true, true).await;
    // b has no consent: their import is skipped, a's consent does not leak.
    let (_, res) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("same")),
        Some(&b),
    )
    .await;
    assert_eq!(res["steps"]["skipped"], 2);
    consent(&app, &b, true, true, true).await;

    let (_, ra) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("same")),
        Some(&a),
    )
    .await;
    let (_, rb) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(payload("same")),
        Some(&b),
    )
    .await;
    // The same external ids import for both users; neither collides.
    assert_eq!(ra["steps"]["imported"], 2);
    assert_eq!(rb["steps"]["imported"], 2);
    assert_eq!(rb["workouts"]["imported"], 1);
    assert_eq!(rb["sleep"]["imported"], 1);
    for (t, expected) in [("health_samples", 4), ("workouts", 2), ("sleep_logs", 2)] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {t}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, expected, "{t}");
    }
    // A's consent revocation does not affect B.
    consent(&app, &a, false, false, false).await;
    let (_, cb) = call(&app, Method::GET, "/health/consents", None, Some(&b)).await;
    assert_eq!(
        cb,
        json!({ "steps": true, "workouts": true, "sleep": true })
    );
}

#[sqlx::test]
async fn caps(pool: PgPool) {
    let app = router(pool);
    let tok = register(&app, "a@example.com").await;
    consent(&app, &tok, true, true, true).await;
    let today = Utc::now().date_naive().to_string();
    let items = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|i| json!({ "external_id": format!("s{i}"), "date": today, "count": 1 }))
            .collect()
    };
    let (s, b) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(json!({ "steps": items(1000) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["steps"]["imported"], 1000);
    let (s, _) = call(
        &app,
        Method::POST,
        "/health/import",
        Some(json!({ "steps": items(1001) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

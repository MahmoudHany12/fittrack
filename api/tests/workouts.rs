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

/// Register a user and return their access token.
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

/// Id of a catalog exercise by name.
async fn exercise(app: &Router, token: &str, name: &str) -> String {
    let (status, body) = call(
        app,
        Method::GET,
        &format!("/exercises?q={}", name.replace(' ', "%20")),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body.as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("{name} not in catalog"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn workout_body(ex: &str) -> Value {
    json!({
        "duration_s": 1800,
        "note": "Push day",
        "kind": " strength ",
        "sets": [
            { "exercise_id": ex, "reps": 8, "weight_kg": 60.0 },
            { "exercise_id": ex, "reps": 6, "weight_kg": 65.5 },
        ],
    })
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    for (m, uri) in [
        (Method::GET, "/exercises"),
        (Method::GET, "/workouts"),
        (Method::POST, "/workouts"),
    ] {
        let (s, _) = call(&app, m, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[sqlx::test]
async fn create_list_update_delete(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let bench = exercise(&app, &t, "Bench Press").await;
    let squat = exercise(&app, &t, "Squat").await;

    let (s, created) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(workout_body(&bench)),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["duration_s"], 1800);
    assert_eq!(created["source"], "manual");
    assert_eq!(created["kind"], "strength");
    assert_eq!(created["sets"].as_array().unwrap().len(), 2);
    assert_eq!(created["sets"][0]["position"], 0);
    assert_eq!(created["sets"][1]["weight_kg"], 65.5);

    let (s, list) = call(&app, Method::GET, "/workouts", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["sets"].as_array().unwrap().len(), 2);

    // PUT replaces the sets.
    let (s, updated) = call(
        &app,
        Method::PUT,
        &format!("/workouts/{id}"),
        Some(json!({
            "duration_s": 2400,
            "sets": [{ "exercise_id": squat, "reps": 5, "weight_kg": 100.0 }],
        })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{updated}");
    assert_eq!(updated["duration_s"], 2400);
    assert!(updated["note"].is_null());
    // Omitting kind on PUT keeps the stored kind.
    assert_eq!(updated["kind"], "strength");
    assert_eq!(updated["sets"].as_array().unwrap().len(), 1);
    assert_eq!(updated["sets"][0]["exercise_id"], squat);

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/workouts/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, Method::GET, "/workouts", None, Some(&t)).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    // Deleted workouts behave as missing.
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/workouts/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/workouts/{id}"),
        Some(workout_body(&bench)),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_filters_by_date_range(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let ex = exercise(&app, &t, "Running").await;
    for day in [
        "2025-01-10T08:00:00Z",
        "2025-02-10T08:00:00Z",
        "2025-03-10T08:00:00Z",
    ] {
        let mut b = workout_body(&ex);
        b["started_at"] = json!(day);
        let (s, _) = call(&app, Method::POST, "/workouts", Some(b), Some(&t)).await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (s, list) = call(
        &app,
        Method::GET,
        "/workouts?from=2025-02-01T00:00:00Z&to=2025-02-28T00:00:00Z",
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["started_at"], "2025-02-10T08:00:00Z");
}

#[sqlx::test]
async fn validation_errors(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let ex = exercise(&app, &t, "Squat").await;

    let cases = [
        ("duration_s", json!({ "duration_s": 0, "sets": [] })),
        ("duration_s", json!({ "duration_s": 86401, "sets": [] })),
        (
            "sets",
            json!({ "duration_s": 60, "sets": [{ "exercise_id": ex, "reps": 0, "weight_kg": 10 }] }),
        ),
        (
            "sets",
            json!({ "duration_s": 60, "sets": [{ "exercise_id": ex, "reps": 1001, "weight_kg": 10 }] }),
        ),
        (
            "sets",
            json!({ "duration_s": 60, "sets": [{ "exercise_id": ex, "reps": 5, "weight_kg": -1 }] }),
        ),
        (
            "sets",
            json!({ "duration_s": 60, "sets": [{ "exercise_id": ex, "reps": 5, "weight_kg": 1000.5 }] }),
        ),
        (
            "sets",
            json!({
                "duration_s": 60,
                "sets": [{ "exercise_id": "00000000-0000-0000-0000-000000000000", "reps": 5, "weight_kg": 10 }],
            }),
        ),
        ("note", json!({ "duration_s": 60, "note": "x".repeat(501) })),
        ("kind", json!({ "duration_s": 60, "kind": "k".repeat(41) })),
        ("kind", json!({ "duration_s": 60, "kind": "   " })),
    ];
    for (field, body) in cases {
        let (s, b) = call(
            &app,
            Method::POST,
            "/workouts",
            Some(body.clone()),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body} -> {b}");
        assert!(b["error"]["fields"][field].is_array(), "{field}: {b}");
    }

    // Missing required field and malformed ids are bad requests.
    let (s, _) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(json!({ "sets": [] })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(json!({ "id": "nope", "duration_s": 60 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Nothing was stored.
    let (_, list) = call(&app, Method::GET, "/workouts", None, Some(&t)).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn other_users_data_is_invisible(pool: PgPool) {
    let app = router(pool);
    let a = register(&app, "ann@example.com").await;
    let b = register(&app, "bob@example.com").await;
    let ex = exercise(&app, &a, "Deadlift").await;

    let (_, created) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(workout_body(&ex)),
        Some(&a),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let (_, list) = call(&app, Method::GET, "/workouts", None, Some(&b)).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/workouts/{id}"),
        Some(workout_body(&ex)),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/workouts/{id}"),
        None,
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Still intact for the owner.
    let (_, list) = call(&app, Method::GET, "/workouts", None, Some(&a)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["duration_s"], 1800);
}

#[sqlx::test]
async fn cannot_use_another_users_custom_exercise(pool: PgPool) {
    let app = router(pool);
    let a = register(&app, "ann@example.com").await;
    let b = register(&app, "bob@example.com").await;
    let (s, custom) = call(
        &app,
        Method::POST,
        "/exercises",
        Some(json!({ "name": "Ann's Special", "muscle_group": "core" })),
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let cid = custom["id"].as_str().unwrap();

    let (s, _) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(workout_body(cid)),
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, body) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(workout_body(cid)),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[sqlx::test]
async fn create_with_same_client_id_is_idempotent(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let ex = exercise(&app, &t, "Bench Press").await;
    let id = "11111111-2222-4333-8444-555555555555";

    let mut body = workout_body(&ex);
    body["id"] = json!(id);
    let (s, first) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(body.clone()),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(first["id"], id);

    // Exact retry: no duplicate, sets not doubled.
    let (s, _) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(body.clone()),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Retry with edits updates in place.
    body["duration_s"] = json!(999);
    let (s, again) = call(&app, Method::POST, "/workouts", Some(body), Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again["duration_s"], 999);

    let (_, list) = call(&app, Method::GET, "/workouts", None, Some(&t)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["sets"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn client_id_cannot_overwrite_another_users_workout(pool: PgPool) {
    let app = router(pool);
    let a = register(&app, "ann@example.com").await;
    let b = register(&app, "bob@example.com").await;
    let ex = exercise(&app, &a, "Bench Press").await;
    let id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    let mut body = workout_body(&ex);
    body["id"] = json!(id);
    let (s, _) = call(
        &app,
        Method::POST,
        "/workouts",
        Some(body.clone()),
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    body["duration_s"] = json!(1);
    body["note"] = json!("hijacked");
    let (s, _) = call(&app, Method::POST, "/workouts", Some(body), Some(&b)).await;
    assert!(
        s == StatusCode::NOT_FOUND || s == StatusCode::CONFLICT,
        "unexpected {s}"
    );

    let (_, a_list) = call(&app, Method::GET, "/workouts", None, Some(&a)).await;
    assert_eq!(a_list[0]["duration_s"], 1800);
    assert_eq!(a_list[0]["note"], "Push day");
    let (_, b_list) = call(&app, Method::GET, "/workouts", None, Some(&b)).await;
    assert_eq!(b_list.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn exercise_search_and_custom(pool: PgPool) {
    let app = router(pool);
    let a = register(&app, "ann@example.com").await;
    let b = register(&app, "bob@example.com").await;

    let (s, all) = call(&app, Method::GET, "/exercises", None, Some(&a)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(all.as_array().unwrap().len() >= 60);

    let (_, found) = call(&app, Method::GET, "/exercises?q=bench", None, Some(&a)).await;
    let names: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Bench Press"));
    assert!(names.iter().all(|n| n.to_lowercase().contains("bench")));

    // Wildcards are literal.
    let (_, none) = call(&app, Method::GET, "/exercises?q=%25", None, Some(&a)).await;
    assert_eq!(none.as_array().unwrap().len(), 0);

    let (s, c) = call(
        &app,
        Method::POST,
        "/exercises",
        Some(json!({ "name": "  Sled Push ", "muscle_group": "legs" })),
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{c}");
    assert_eq!(c["name"], "Sled Push");
    assert_eq!(c["custom"], true);

    // Visible to the owner only.
    let (_, mine) = call(&app, Method::GET, "/exercises?q=sled", None, Some(&a)).await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let (_, theirs) = call(&app, Method::GET, "/exercises?q=sled", None, Some(&b)).await;
    assert_eq!(theirs.as_array().unwrap().len(), 0);

    // Duplicates (own or catalog, case-insensitive) conflict.
    for name in ["sled push", "BENCH PRESS"] {
        let (s, e) = call(
            &app,
            Method::POST,
            "/exercises",
            Some(json!({ "name": name, "muscle_group": "legs" })),
            Some(&a),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT, "{name}: {e}");
    }
    // Another user may reuse the name.
    let (s, _) = call(
        &app,
        Method::POST,
        "/exercises",
        Some(json!({ "name": "Sled Push", "muscle_group": "legs" })),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
}

#[sqlx::test]
async fn custom_exercise_validation(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    for (field, body) in [
        ("name", json!({ "name": "", "muscle_group": "legs" })),
        ("name", json!({ "name": "   ", "muscle_group": "legs" })),
        (
            "name",
            json!({ "name": "x".repeat(81), "muscle_group": "legs" }),
        ),
        (
            "muscle_group",
            json!({ "name": "Thing", "muscle_group": "nose" }),
        ),
    ] {
        let (s, b) = call(
            &app,
            Method::POST,
            "/exercises",
            Some(body.clone()),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(b["error"]["fields"][field].is_array(), "{b}");
    }
}

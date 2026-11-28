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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
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

fn scan(id: Uuid) -> Value {
    json!({
        "id": id,
        "weight_kg": 80.5,
        "body_fat_pct": 18.2,
        "skeletal_muscle_kg": 38.1,
        "fat_mass_kg": 14.6,
        "visceral_fat_level": 6,
        "bmr_kcal": 1800,
        "body_water_pct": 55.0,
        "source": "inbody"
    })
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/body-composition", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, Method::GET, "/targets", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn create_is_idempotent_and_mirrors_weight(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();

    let (s, b) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["id"], id.to_string());
    assert_eq!(b["body_fat_pct"], 18.2);
    assert_eq!(b["source"], "inbody");

    // Replay: same id updates instead of duplicating.
    let mut again = scan(id);
    again["weight_kg"] = json!(79.9);
    let (s, b) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(again),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["weight_kg"], 79.9);

    let (_, list) = call(&app, Method::GET, "/body-composition", None, Some(&t)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // The weight log has the same id and the latest kg, exactly once.
    let (_, w) = call(&app, Method::GET, "/weight", None, Some(&t)).await;
    let w = w.as_array().unwrap();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0]["id"], id.to_string());
    assert_eq!(w[0]["kg"], 79.9);
}

#[sqlx::test]
async fn manual_entry_defaults_and_listing_order(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let (s, b) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({ "weight_kg": 70.0, "measured_at": "2026-01-01T08:00:00Z" })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["source"], "manual");
    assert!(b["body_fat_pct"].is_null());
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({ "weight_kg": 71.0, "measured_at": "2026-02-01T08:00:00Z" })),
        Some(&t),
    )
    .await;
    let (_, list) = call(&app, Method::GET, "/body-composition", None, Some(&t)).await;
    assert_eq!(list[0]["weight_kg"], 71.0);
    assert_eq!(list[1]["weight_kg"], 70.0);
    let (_, list) = call(
        &app,
        Method::GET,
        "/body-composition?from=2026-01-15T00:00:00Z",
        None,
        Some(&t),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn put_updates_both_rows(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&t),
    )
    .await;
    let (s, b) = call(
        &app,
        Method::PUT,
        &format!("/body-composition/{id}"),
        Some(json!({ "weight_kg": 78.0, "body_fat_pct": 17.0 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["weight_kg"], 78.0);
    assert_eq!(b["body_fat_pct"], 17.0);
    assert!(b["skeletal_muscle_kg"].is_null());
    let (_, w) = call(&app, Method::GET, "/weight", None, Some(&t)).await;
    assert_eq!(w[0]["kg"], 78.0);
}

#[sqlx::test]
async fn delete_soft_deletes_both(pool: PgPool) {
    let app = router(pool.clone());
    let t = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&t),
    )
    .await;
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/body-composition/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, Method::GET, "/body-composition", None, Some(&t)).await;
    assert!(list.as_array().unwrap().is_empty());
    let (_, w) = call(&app, Method::GET, "/weight", None, Some(&t)).await;
    assert!(w.as_array().unwrap().is_empty());
    // Rows are kept (soft delete).
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM body_compositions WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // Second delete and updates are 404.
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/body-composition/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn other_users_rows_are_not_visible_or_modifiable(pool: PgPool) {
    let app = router(pool);
    let a = register(&app, "ann@example.com").await;
    let b = register(&app, "bob@example.com").await;
    let id = Uuid::new_v4();
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&a),
    )
    .await;

    let (_, list) = call(&app, Method::GET, "/body-composition", None, Some(&b)).await;
    assert!(list.as_array().unwrap().is_empty());
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/body-composition/{id}"),
        Some(json!({ "weight_kg": 60.0 })),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/body-composition/{id}"),
        None,
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Re-posting someone else's id must not hijack it.
    let (s, _) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(scan(id)),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, mine) = call(&app, Method::GET, "/body-composition", None, Some(&a)).await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let (_, w) = call(&app, Method::GET, "/weight", None, Some(&b)).await;
    assert!(w.as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn validation_rejects_out_of_range_values(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let bad = [
        json!({ "weight_kg": 10.0 }),
        json!({ "weight_kg": 401.0 }),
        json!({ "weight_kg": 80.0, "body_fat_pct": 1.0 }),
        json!({ "weight_kg": 80.0, "body_fat_pct": 76.0 }),
        json!({ "weight_kg": 80.0, "skeletal_muscle_kg": 4.0 }),
        json!({ "weight_kg": 80.0, "skeletal_muscle_kg": 101.0 }),
        json!({ "weight_kg": 80.0, "visceral_fat_level": 0 }),
        json!({ "weight_kg": 80.0, "visceral_fat_level": 31 }),
        json!({ "weight_kg": 80.0, "bmr_kcal": 599 }),
        json!({ "weight_kg": 80.0, "bmr_kcal": 5001 }),
        json!({ "weight_kg": 80.0, "body_water_pct": 19.0 }),
        json!({ "weight_kg": 80.0, "body_water_pct": 81.0 }),
        json!({ "weight_kg": 80.0, "source": "scale" }),
    ];
    for body in bad {
        let (s, r) = call(
            &app,
            Method::POST,
            "/body-composition",
            Some(body.clone()),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body} -> {r}");
    }
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/body-composition/{}", Uuid::new_v4()),
        Some(json!({ "weight_kg": 5.0 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, list) = call(&app, Method::GET, "/body-composition", None, Some(&t)).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn profile_body_fields_validate_and_persist(pool: PgPool) {
    let app = router(pool);
    let t = register(&app, "ann@example.com").await;
    let base = |extra: Value| {
        let mut b = json!({ "calorie_target": 2000, "units": "metric" });
        for (k, v) in extra.as_object().unwrap() {
            b[k] = v.clone();
        }
        b
    };
    for extra in [
        json!({ "height_cm": 99.0 }),
        json!({ "height_cm": 251.0 }),
        json!({ "sex": "other" }),
        json!({ "activity_level": "lazy" }),
        json!({ "goal": "bulk" }),
        json!({ "goal_rate_kg_per_week": 1.1 }),
        json!({ "goal_rate_kg_per_week": -0.1 }),
        json!({ "calorie_target_mode": "magic" }),
        json!({ "birth_date": "2020-01-01" }),
        json!({ "birth_date": "1900-01-01" }),
    ] {
        let (s, r) = call(
            &app,
            Method::PUT,
            "/profile",
            Some(base(extra.clone())),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{extra} -> {r}");
    }
    let (s, p) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(base(json!({
            "height_cm": 180.0, "birth_date": "1990-05-20", "sex": "male",
            "activity_level": "active", "goal": "lose", "goal_rate_kg_per_week": 0.75,
            "calorie_target_mode": "manual"
        }))),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["height_cm"], 180.0);
    assert_eq!(p["sex"], "male");
    assert_eq!(p["calorie_target_mode"], "manual");

    // A later PUT without the new fields keeps them.
    let (_, p) = call(
        &app,
        Method::PUT,
        "/profile",
        Some(base(json!({}))),
        Some(&t),
    )
    .await;
    assert_eq!(p["height_cm"], 180.0);
    assert_eq!(p["goal"], "lose");
    assert_eq!(p["calorie_target_mode"], "manual");
    let (_, p) = call(&app, Method::GET, "/profile", None, Some(&t)).await;
    assert_eq!(p["birth_date"], "1990-05-20");
}

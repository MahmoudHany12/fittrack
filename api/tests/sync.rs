use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
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

async fn sync(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/sync")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

fn ts(offset_s: i64) -> String {
    (Utc::now() + Duration::seconds(offset_s)).to_rfc3339()
}

fn weight(id: Uuid, kg: f64, updated_offset_s: i64) -> Value {
    json!({ "id": id, "updated_at": ts(updated_offset_s), "measured_at": ts(-60), "kg": kg })
}

fn first_weight(resp: &Value) -> &Value {
    &resp["changes"]["weight"][0]
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    let req = Request::builder()
        .method("POST")
        .uri("/sync")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let (s, _) = send(&app, req).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn push_all_tables_then_pull_everything(pool: PgPool) {
    let app = router(pool.clone());
    let (token, uid) = register(&app, "ann@example.com").await;
    let exercise: (Uuid,) =
        sqlx::query_as("SELECT id FROM exercises WHERE owner_id IS NULL LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let food: (Uuid,) = sqlx::query_as(
        "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ('Rice', 500, 10, 100, 1, 150, 'custom', $1) RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (wid, sid, xid, fid) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );

    let (s, r) = sync(
        &app,
        &token,
        json!({
            "since": null,
            "changes": {
                "workouts": [{
                    "id": wid, "updated_at": ts(-10), "started_at": ts(-3600), "duration_s": 1800,
                    "note": "legs", "sets": [{ "exercise_id": exercise.0, "reps": 5, "weight_kg": 100.0 }]
                }],
                "sleep": [{ "id": sid, "updated_at": ts(-10), "bedtime": ts(-30000), "wake_time": ts(-1200), "quality": 4 }],
                "weight": [weight(xid, 78.4, -10)],
                "food_logs": [{
                    "id": fid, "updated_at": ts(-10), "food_id": food.0, "logged_at": ts(-600),
                    "servings": 2, "kcal": 1000, "protein_g": 20, "carbs_g": 200, "fat_g": 2, "meal": "lunch"
                }]
            }
        }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    for t in ["workouts", "sleep", "weight", "food_logs"] {
        assert_eq!(r["applied"][t]["applied"], 1, "{t}: {r}");
    }
    assert!(r["rejected"].as_array().unwrap().is_empty());
    assert_eq!(r["changes"]["workouts"][0]["sets"][0]["reps"], 5);
    assert_eq!(r["changes"]["weight"][0]["kg"], 78.4);
    assert_eq!(r["changes"]["food_logs"][0]["kcal"], 1000.0);
    assert_eq!(r["changes"]["sleep"][0]["quality"], 4);
    assert_eq!(r["has_more"], false);

    // The pushed workout is a normal workout for the REST API too.
    let req = Request::builder()
        .uri("/workouts")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (s, list) = send(&app, req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(list.to_string().contains(&wid.to_string()));
}

#[sqlx::test]
async fn last_write_wins(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();

    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 80.0, -100)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);

    // Newer wins.
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 79.0, -50)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);
    assert_eq!(first_weight(&r)["kg"], 79.0);

    // Older is ignored.
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 90.0, -500)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["ignored"], 1);
    assert_eq!(r["applied"]["weight"]["applied"], 0);

    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    assert_eq!(first_weight(&r)["kg"], 79.0);
}

#[sqlx::test]
async fn equal_timestamp_keeps_server_copy(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();
    let t = ts(-100);
    let row = |kg: f64| json!({ "id": id, "updated_at": t, "measured_at": ts(-60), "kg": kg });
    sync(&app, &token, json!({"changes": {"weight": [row(80.0)]}})).await;
    let (_, r) = sync(&app, &token, json!({"changes": {"weight": [row(70.0)]}})).await;
    assert_eq!(r["applied"]["weight"]["ignored"], 1);
    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    assert_eq!(first_weight(&r)["kg"], 80.0);
}

#[sqlx::test]
async fn delete_then_newer_edit_resurrects(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();
    sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 80.0, -300)]}}),
    )
    .await;

    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [{"id": id, "updated_at": ts(-200), "deleted": true}]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);
    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    assert_eq!(first_weight(&r)["deleted"], true);

    // A stale edit made before the delete does not resurrect it.
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 81.0, -250)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["ignored"], 1);

    // A newer edit does.
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 82.0, -50)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);
    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    assert_eq!(first_weight(&r)["deleted"], false);
    assert_eq!(first_weight(&r)["kg"], 82.0);
}

#[sqlx::test]
async fn deleting_an_unknown_id_is_ignored(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let (s, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [{"id": Uuid::new_v4(), "updated_at": ts(-5), "deleted": true}]}}),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["applied"]["weight"]["ignored"], 1);
}

#[sqlx::test]
async fn users_cannot_touch_or_see_each_others_rows(pool: PgPool) {
    let app = router(pool);
    let (ann, _) = register(&app, "ann@example.com").await;
    let (bob, _) = register(&app, "bob@example.com").await;
    let id = Uuid::new_v4();
    sync(
        &app,
        &ann,
        json!({"changes": {"weight": [weight(id, 60.0, -100)]}}),
    )
    .await;

    // Bob tries to overwrite and to delete Ann's row with newer timestamps.
    let (_, r) = sync(
        &app,
        &bob,
        json!({"changes": {"weight": [weight(id, 99.0, -10)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["ignored"], 1);
    assert!(r["changes"]["weight"].as_array().unwrap().is_empty());
    let (_, r) = sync(
        &app,
        &bob,
        json!({"changes": {"weight": [{"id": id, "updated_at": ts(-5), "deleted": true}]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["ignored"], 1);

    let (_, r) = sync(&app, &ann, json!({"since": null, "changes": {}})).await;
    assert_eq!(first_weight(&r)["kg"], 60.0);
    assert_eq!(first_weight(&r)["deleted"], false);
}

#[sqlx::test]
async fn invalid_rows_are_rejected_but_valid_rows_still_apply(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let good = Uuid::new_v4();
    let bad_kg = Uuid::new_v4();
    let bad_sleep = Uuid::new_v4();
    let (s, r) = sync(
        &app,
        &token,
        json!({"changes": {
            "weight": [weight(good, 70.0, -10), weight(bad_kg, 5.0, -10)],
            "sleep": [{ "id": bad_sleep, "updated_at": ts(-10), "bedtime": ts(-100), "wake_time": ts(-5000) }]
        }}),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["applied"]["weight"]["applied"], 1);
    let rejected = r["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 2);
    assert!(rejected.iter().any(|x| x["id"] == bad_kg.to_string()));
    assert!(rejected.iter().any(|x| x["table"] == "sleep"));
}

#[sqlx::test]
async fn foreign_references_are_rejected(pool: PgPool) {
    let app = router(pool.clone());
    let (_ann, ann_id) = register(&app, "ann@example.com").await;
    let (bob, _) = register(&app, "bob@example.com").await;
    let food: (Uuid,) = sqlx::query_as(
        "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ('Secret', 100, 1, 1, 1, 100, 'custom', $1) RETURNING id",
    )
    .bind(ann_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Bob logs Ann's private custom food.
    let (_, r) = sync(
        &app,
        &bob,
        json!({"changes": {"food_logs": [{
            "id": Uuid::new_v4(), "updated_at": ts(-5), "food_id": food.0, "logged_at": ts(-60),
            "servings": 1, "kcal": 100, "protein_g": 1, "carbs_g": 1, "fat_g": 1
        }]}}),
    )
    .await;
    assert_eq!(r["rejected"].as_array().unwrap().len(), 1);
    // Bob uses an exercise id that doesn't exist.
    let (_, r) = sync(
        &app,
        &bob,
        json!({"changes": {"workouts": [{
            "id": Uuid::new_v4(), "updated_at": ts(-5), "started_at": ts(-3600), "duration_s": 60,
            "sets": [{ "exercise_id": Uuid::new_v4(), "reps": 1, "weight_kg": 1.0 }]
        }]}}),
    )
    .await;
    assert_eq!(r["rejected"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn pull_cursor_delivers_late_offline_edits_to_other_devices(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;

    // Device A syncs and stores the cursor.
    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    let cursor: DateTime<Utc> = r["server_time"].as_str().unwrap().parse().unwrap();

    // Device B uploads an edit it made *long ago* (old updated_at).
    let id = Uuid::new_v4();
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [weight(id, 71.0, -86_400)]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);

    // Device A pulls with its cursor and still receives it.
    let (_, r) = sync(&app, &token, json!({"since": cursor, "changes": {}})).await;
    assert_eq!(first_weight(&r)["kg"], 71.0);

    // A second pull from the new cursor returns nothing.
    let cursor: DateTime<Utc> = r["server_time"].as_str().unwrap().parse().unwrap();
    let (_, r) = sync(&app, &token, json!({"since": cursor, "changes": {}})).await;
    assert!(r["changes"]["weight"].as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn future_timestamps_are_clamped_to_server_time(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();
    // A device with a clock one year ahead cannot lock the row forever.
    let far = ts(365 * 86_400);
    let row =
        |kg: f64, at: &str| json!({ "id": id, "updated_at": at, "measured_at": ts(-60), "kg": kg });
    sync(
        &app,
        &token,
        json!({"changes": {"weight": [row(100.0, &far)]}}),
    )
    .await;
    let (_, r) = sync(&app, &token, json!({"since": null, "changes": {}})).await;
    let stored: DateTime<Utc> = first_weight(&r)["updated_at"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(stored <= Utc::now() + Duration::seconds(5));
    // A correct-clock edit made after that moment wins.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_, r) = sync(
        &app,
        &token,
        json!({"changes": {"weight": [row(70.0, &ts(0))]}}),
    )
    .await;
    assert_eq!(r["applied"]["weight"]["applied"], 1);
}

#[sqlx::test]
async fn too_many_changes_are_rejected(pool: PgPool) {
    let app = router(pool);
    let (token, _) = register(&app, "ann@example.com").await;
    let rows: Vec<Value> = (0..501)
        .map(|_| weight(Uuid::new_v4(), 70.0, -10))
        .collect();
    let (s, _) = sync(&app, &token, json!({"changes": {"weight": rows}})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

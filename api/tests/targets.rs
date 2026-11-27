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

async fn put_profile(app: &Router, t: &str, extra: Value) {
    let mut b = json!({ "calorie_target": 2200, "units": "metric" });
    for (k, v) in extra.as_object().unwrap() {
        b[k] = v.clone();
    }
    let (s, r) = call(app, Method::PUT, "/profile", Some(b), Some(t)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
}

async fn weigh(app: &Router, t: &str, kg: f64) {
    let (s, r) = call(
        app,
        Method::POST,
        "/weight",
        Some(json!({ "kg": kg })),
        Some(t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
}

fn birth_date_for_age(age: i32) -> String {
    use chrono::Datelike;
    let today = chrono::Utc::now().date_naive();
    // Safely before this year's birthday: Jan 1 of (year - age), minus nothing.
    format!("{}-01-01", today.year() - age)
}

#[sqlx::test]
async fn no_profile_defaults_to_2200(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    let (s, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["calorie_target"], 2200);
    assert_eq!(r["calorie_target_source"], "default");
    assert!(r["bmr"].is_null());
    assert!(r["tdee"].is_null());
    assert!(r["protein_g"].is_null());
    let missing: Vec<&str> = r["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(missing.contains(&"height_cm"));
    assert!(missing.contains(&"sex"));
}

#[sqlx::test]
async fn mifflin_inputs_give_auto_target(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    // Jan 1 birth: age is exactly 30 all year.
    put_profile(
        &app,
        &t,
        json!({
            "height_cm": 180.0, "birth_date": birth_date_for_age(30), "sex": "male",
            "activity_level": "moderate", "goal": "lose"
        }),
    )
    .await;
    weigh(&app, &t, 80.0).await;
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["bmr"], 1780);
    assert_eq!(r["bmr_source"], "mifflin");
    assert_eq!(r["tdee"], 2759);
    assert_eq!(r["tdee_source"], "formula");
    assert_eq!(r["calorie_target"], 2210);
    assert_eq!(r["calorie_target_source"], "auto");
    assert_eq!(r["protein_g"], 128);
    assert!(r["lean_mass_kg"].is_null());
    assert!(r["missing"].as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn inbody_scan_takes_precedence(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    put_profile(
        &app,
        &t,
        json!({
            "height_cm": 180.0, "birth_date": birth_date_for_age(30), "sex": "male",
            "activity_level": "sedentary", "goal": "maintain"
        }),
    )
    .await;
    let (s, r) = call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({
            "weight_kg": 80.0, "body_fat_pct": 25.0, "skeletal_muscle_kg": 35.0,
            "bmr_kcal": 1800, "source": "inbody"
        })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["bmr"], 1800);
    assert_eq!(r["bmr_source"], "inbody");
    assert_eq!(r["tdee"], 2160);
    assert_eq!(r["calorie_target"], 2160);
    assert_eq!(r["lean_mass_kg"], 60.0);
    assert_eq!(r["body_fat_pct"], 25.0);
    assert_eq!(r["protein_g"], 120);
}

#[sqlx::test]
async fn katch_mcardle_when_scan_has_no_bmr(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({ "weight_kg": 80.0, "body_fat_pct": 25.0 })),
        Some(&t),
    )
    .await;
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["bmr_source"], "katch_mcardle");
    assert_eq!(r["bmr"], 1666);
}

#[sqlx::test]
async fn old_scans_are_ignored(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    let old = (chrono::Utc::now() - chrono::Duration::days(120)).to_rfc3339();
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({ "weight_kg": 80.0, "body_fat_pct": 25.0, "bmr_kcal": 1800, "measured_at": old })),
        Some(&t),
    )
    .await;
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert!(r["bmr"].is_null(), "{r}");
    assert_eq!(r["calorie_target_source"], "default");
}

#[sqlx::test]
async fn manual_mode_keeps_profile_value(pool: PgPool) {
    let app = router(pool);
    let (t, _) = register(&app, "ann@example.com").await;
    put_profile(
        &app,
        &t,
        json!({
            "calorie_target": 1850, "height_cm": 180.0, "birth_date": birth_date_for_age(30),
            "sex": "male", "goal": "lose", "calorie_target_mode": "manual"
        }),
    )
    .await;
    weigh(&app, &t, 80.0).await;
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["calorie_target"], 1850);
    assert_eq!(r["calorie_target_source"], "manual");
    assert_eq!(r["bmr"], 1780);
    assert_eq!(r["protein_g"], 128);
}

#[sqlx::test]
async fn targets_are_per_user(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;
    call(
        &app,
        Method::POST,
        "/body-composition",
        Some(json!({ "weight_kg": 80.0, "bmr_kcal": 1800 })),
        Some(&a),
    )
    .await;
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&b)).await;
    assert!(r["bmr"].is_null());
}

#[sqlx::test]
async fn adaptive_tdee_blends_with_enough_data(pool: PgPool) {
    let app = router(pool.clone());
    let (t, uid) = register(&app, "ann@example.com").await;
    put_profile(
        &app,
        &t,
        json!({
            "height_cm": 180.0, "birth_date": birth_date_for_age(30), "sex": "male",
            "activity_level": "moderate"
        }),
    )
    .await;
    // Weights 19 days apart; flat weight and 3000 kcal/day intake on 16 days
    // -> adaptive 3000, formula 2759, blended 2879.5 (within the +-25% clamp).
    for (days, kg) in [(20, 80.0_f64), (1, 80.0)] {
        sqlx::query(
            "INSERT INTO weight_logs (user_id, measured_at, kg) \
             VALUES ($1, now() - make_interval(days => $2), $3::float8::numeric(5,2))",
        )
        .bind(uid)
        .bind(days)
        .bind(kg)
        .execute(&pool)
        .await
        .unwrap();
    }
    let food: Uuid = sqlx::query_scalar(
        "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ('Test', 3000, 100, 300, 100, 100, 'custom', $1) RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    for d in 0..16 {
        sqlx::query(
            "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g) \
             VALUES (gen_random_uuid(), $1, $2, now() - make_interval(days => $3), 1, 3000, 100, 300, 100)",
        )
        .bind(uid)
        .bind(food)
        .bind(d)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["tdee_source"], "blended", "{r}");
    let tdee = r["tdee"].as_i64().unwrap();
    assert!((2878..=2881).contains(&tdee), "{tdee}");

    // Insufficient data (food days < 14) falls back to the formula.
    sqlx::query(
        "DELETE FROM food_logs WHERE user_id = $1 AND logged_at < now() - interval '5 days'",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    let (_, r) = call(&app, Method::GET, "/targets", None, Some(&t)).await;
    assert_eq!(r["tdee_source"], "formula");
    assert_eq!(r["tdee"], 2759);
}

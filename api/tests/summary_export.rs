use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use fittrack_api::{app, config::Config, summary::weekly_summary, AppState};
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

struct Resp {
    status: StatusCode,
    content_type: String,
    disposition: String,
    text: String,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or(Value::Null)
    }
}

async fn raw(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    tok: Option<&str>,
) -> Resp {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = tok {
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
    let h = |n: &str| {
        resp.headers()
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (content_type, disposition) = (h("content-type"), h("content-disposition"));
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        content_type,
        disposition,
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn register(app: &Router, email: &str) -> (String, Uuid) {
    let r = raw(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({ "email": email, "password": "correct-horse-battery" })),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let token = r.json()["access_token"].as_str().unwrap().to_string();
    let me = raw(app, Method::GET, "/me", None, Some(&token))
        .await
        .json();
    (token, me["id"].as_str().unwrap().parse().unwrap())
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn at(d: NaiveDate, hour: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(hour, 0, 0).unwrap())
}

async fn add_workout(pool: &PgPool, user: Uuid, when: DateTime<Utc>, minutes: i32) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workouts (user_id, started_at, duration_s, note) \
         VALUES ($1, $2, $3, '=cmd|calc') RETURNING id",
    )
    .bind(user)
    .bind(when)
    .bind(minutes * 60)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

async fn add_sleep(pool: &PgPool, user: Uuid, wake: DateTime<Utc>, hours: i64, quality: i16) {
    sqlx::query(
        "INSERT INTO sleep_logs (user_id, bedtime, wake_time, quality) VALUES ($1, $2, $3, $4)",
    )
    .bind(user)
    .bind(wake - Duration::hours(hours))
    .bind(wake)
    .bind(quality)
    .execute(pool)
    .await
    .unwrap();
}

async fn add_weight(pool: &PgPool, user: Uuid, when: DateTime<Utc>, kg: f64) {
    sqlx::query("INSERT INTO weight_logs (user_id, measured_at, kg) VALUES ($1, $2, $3::float8::numeric(5,2))")
        .bind(user)
        .bind(when)
        .bind(kg)
        .execute(pool)
        .await
        .unwrap();
}

async fn add_food(pool: &PgPool, user: Uuid, name: &str, when: DateTime<Utc>, kcal: f64) {
    let (food,): (Uuid,) = sqlx::query_as(
        "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ($1, $2, 1, 1, 1, 100, 'custom', $3) RETURNING id",
    )
    .bind(name)
    .bind(kcal)
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g) \
         VALUES ($1, $2, $3, $4, 1, $5, 1, 1, 1)",
    )
    .bind(Uuid::new_v4())
    .bind(user)
    .bind(food)
    .bind(when)
    .bind(kcal)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn summary_math_across_two_weeks(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    let w1 = day(2026, 3, 2); // previous week: 2..8 March
    let w2 = day(2026, 3, 9); // this week: 9..15 March

    // Previous week: 1 workout of 60 min, 8 h sleep, 80 kg.
    add_workout(&pool, uid, at(w1, 10), 60).await;
    add_sleep(&pool, uid, at(w1, 7), 8, 3).await;
    add_weight(&pool, uid, at(w1, 8), 80.0).await;

    // This week: 3 workouts on 2 days (60 + 40 + 40 = 140 min).
    add_workout(&pool, uid, at(w2, 10), 60).await;
    add_workout(&pool, uid, at(w2 + Duration::days(2), 9), 40).await;
    add_workout(&pool, uid, at(w2 + Duration::days(2), 18), 40).await;
    // Sleep: 7 h (q4) and 8 h (q5) -> avg 450 min, quality 4.5.
    add_sleep(&pool, uid, at(w2 + Duration::days(1), 7), 7, 4).await;
    add_sleep(&pool, uid, at(w2 + Duration::days(3), 7), 8, 5).await;
    // Weight 80 -> 78.5 (down 1.5).
    add_weight(&pool, uid, at(w2, 8), 80.0).await;
    add_weight(&pool, uid, at(w2 + Duration::days(6), 8), 78.5).await;
    // Diet: 2000 + 500 on one day, 1500 on another -> 4000 / 2 days = 2000.
    add_food(&pool, uid, "Rice", at(w2, 12), 2000.0).await;
    add_food(&pool, uid, "Egg", at(w2, 19), 500.0).await;
    add_food(&pool, uid, "Oats", at(w2 + Duration::days(1), 8), 1500.0).await;
    // Outside the window, and soft-deleted rows are ignored.
    add_workout(&pool, uid, at(w2 + Duration::days(7), 0), 500).await;
    sqlx::query("UPDATE workouts SET deleted_at = now() WHERE started_at = $1")
        .bind(at(w2 + Duration::days(2), 18))
        .execute(&pool)
        .await
        .unwrap();

    let r = raw(
        &app,
        Method::GET,
        "/summary/weekly?week_start=2026-03-09",
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let s = r.json();
    assert_eq!(s["from"], "2026-03-09");
    assert_eq!(s["to"], "2026-03-15");
    // One of the three workouts was soft-deleted: 2 workouts, 100 min.
    assert_eq!(s["training"]["workouts"], 2);
    assert_eq!(s["training"]["minutes"], 100);
    assert_eq!(s["training"]["active_days"], 2);
    assert_eq!(s["training"]["vs_prev_week_minutes"], 40);
    assert_eq!(s["sleep"]["nights"], 2);
    assert_eq!(s["sleep"]["avg_minutes"], 450);
    assert_eq!(s["sleep"]["avg_quality"], 4.5);
    assert_eq!(s["sleep"]["vs_prev_week_minutes"], -30);
    assert_eq!(s["weight"]["start_kg"], 80.0);
    assert_eq!(s["weight"]["end_kg"], 78.5);
    assert_eq!(s["weight"]["change_kg"], -1.5);
    assert_eq!(s["weight"]["trend"], "down");
    assert_eq!(s["diet"]["avg_kcal"], 2000);
    assert_eq!(s["diet"]["days_logged"], 2);
    assert!(s["diet"]["target"].is_null()); // no profile row yet

    // The reusable function agrees with the endpoint.
    let direct = weekly_summary(&pool, uid, w2).await.unwrap();
    assert_eq!(direct.training.minutes, Some(100));
    assert_eq!(direct.sleep.avg_minutes, Some(450));
}

#[sqlx::test]
async fn empty_user_gets_nulls(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "empty@example.com").await;
    let r = raw(&app, Method::GET, "/summary/weekly", None, Some(&tok)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let s = r.json();
    for (section, keys) in [
        (
            "training",
            vec!["workouts", "minutes", "active_days", "vs_prev_week_minutes"],
        ),
        (
            "sleep",
            vec![
                "avg_minutes",
                "avg_quality",
                "nights",
                "vs_prev_week_minutes",
            ],
        ),
        ("weight", vec!["start_kg", "end_kg", "change_kg", "trend"]),
        ("diet", vec!["avg_kcal", "days_logged"]),
    ] {
        for k in keys {
            assert!(s[section][k].is_null(), "{section}.{k} should be null: {s}");
        }
    }
    let today = Utc::now().date_naive();
    assert_eq!(s["to"], today.to_string());
    assert_eq!(s["from"], (today - Duration::days(6)).to_string());
}

#[sqlx::test]
async fn summary_only_counts_own_data(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, _) = register(&app, "a@example.com").await;
    let (_, other) = register(&app, "b@example.com").await;
    add_workout(&pool, other, at(day(2026, 3, 10), 9), 45).await;
    let r = raw(
        &app,
        Method::GET,
        "/summary/weekly?week_start=2026-03-09",
        None,
        Some(&tok),
    )
    .await;
    assert!(r.json()["training"]["minutes"].is_null());
}

#[sqlx::test]
async fn export_json_contains_only_own_data(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, me) = register(&app, "mine@example.com").await;
    let (_, other) = register(&app, "theirs@example.com").await;
    let when = at(day(2026, 3, 10), 9);

    let w = add_workout(&pool, me, when, 30).await;
    sqlx::query(
        "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position) \
         SELECT $1, id, 5, 100, 0 FROM exercises WHERE name = 'Squat'",
    )
    .bind(w)
    .execute(&pool)
    .await
    .unwrap();
    add_sleep(&pool, me, when, 8, 4).await;
    add_weight(&pool, me, when, 77.0).await;
    add_food(&pool, me, "My Rice", when, 300.0).await;
    // Soft-deleted rows must not be exported.
    let gone = add_workout(&pool, me, when, 99).await;
    sqlx::query("UPDATE workouts SET deleted_at = now() WHERE id = $1")
        .bind(gone)
        .execute(&pool)
        .await
        .unwrap();

    let ow = add_workout(&pool, other, when, 70).await;
    sqlx::query(
        "INSERT INTO workout_sets (workout_id, exercise_id, reps, weight_kg, position) \
         SELECT $1, id, 8, 50, 0 FROM exercises WHERE name = 'Deadlift'",
    )
    .bind(ow)
    .execute(&pool)
    .await
    .unwrap();
    add_sleep(&pool, other, when, 6, 2).await;
    add_weight(&pool, other, when, 55.0).await;
    add_food(&pool, other, "Their Secret Meal", when, 900.0).await;

    let r = raw(&app, Method::GET, "/export", None, Some(&tok)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert!(r.content_type.starts_with("application/json"));
    assert!(r.disposition.starts_with("attachment"));
    let d = r.json();
    assert_eq!(d["user"]["email"], "mine@example.com");
    assert_eq!(d["workouts"].as_array().unwrap().len(), 1);
    assert_eq!(d["workouts"][0]["duration_s"], 1800);
    assert_eq!(d["workouts"][0]["sets"].as_array().unwrap().len(), 1);
    assert_eq!(d["workouts"][0]["sets"][0]["exercise"], "Squat");
    assert_eq!(d["sleep_logs"].as_array().unwrap().len(), 1);
    assert_eq!(d["weight_logs"].as_array().unwrap().len(), 1);
    assert_eq!(d["weight_logs"][0]["kg"], 77.0);
    assert_eq!(d["food_logs"].as_array().unwrap().len(), 1);
    assert!(d["health_consents"].is_array());
    let text = &r.text;
    for forbidden in [
        "theirs@example.com",
        "Their Secret Meal",
        "Deadlift",
        "password",
        "refresh",
        "token",
    ] {
        assert!(!text.contains(forbidden), "export leaked {forbidden}");
    }

    // Also explicit format=json, and auth is required.
    let r = raw(&app, Method::GET, "/export?format=json", None, Some(&tok)).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = raw(&app, Method::GET, "/export", None, None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn csv_tables_are_scoped_and_escaped(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, me) = register(&app, "csv@example.com").await;
    let (_, other) = register(&app, "other@example.com").await;
    let when = at(day(2026, 3, 10), 9);
    // Notes with formula and with comma+quote.
    add_workout(&pool, me, when, 30).await; // note '=cmd|calc'
    let tricky = add_workout(&pool, me, when + Duration::hours(1), 20).await;
    sqlx::query("UPDATE workouts SET note = $2 WHERE id = $1")
        .bind(tricky)
        .bind("said \"hi\", then left")
        .execute(&pool)
        .await
        .unwrap();
    add_workout(&pool, other, when, 90).await;

    let r = raw(
        &app,
        Method::GET,
        "/export?format=csv&table=workouts",
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert!(r.content_type.starts_with("text/csv"));
    assert!(r.disposition.contains("attachment"));
    assert!(r.disposition.contains("fittrack-workouts.csv"));
    let lines: Vec<&str> = r.text.trim_end().split("\r\n").collect();
    assert_eq!(
        lines[0],
        "id,started_at,duration_s,note,source,external_id,updated_at"
    );
    assert_eq!(lines.len(), 3, "{}", r.text);
    assert!(r.text.contains(",'=cmd|calc,"), "{}", r.text);
    assert!(
        r.text.contains("\"said \"\"hi\"\", then left\""),
        "{}",
        r.text
    );
    assert!(
        !r.text.contains(",5400,"),
        "other user's 90 min workout leaked"
    );

    for t in ["sets", "food", "sleep", "weight"] {
        let r = raw(
            &app,
            Method::GET,
            &format!("/export?format=csv&table={t}"),
            None,
            Some(&tok),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{t}: {}", r.text);
        assert!(r.content_type.starts_with("text/csv"));
    }
}

#[sqlx::test]
async fn bad_params_are_rejected(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "bad@example.com").await;
    for uri in [
        "/export?format=xml",
        "/export?format=csv",
        "/export?format=csv&table=users",
        "/export?format=csv&table=",
        "/export?format=json&table=workouts",
        "/summary/weekly?week_start=yesterday",
        "/summary/weekly?week_start=2026-13-40",
        "/summary/weekly?week_start=1999-01-01",
    ] {
        let r = raw(&app, Method::GET, uri, None, Some(&tok)).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{uri}: {}",
            r.text
        );
    }
    for uri in ["/summary/weekly", "/export"] {
        let r = raw(&app, Method::GET, uri, None, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

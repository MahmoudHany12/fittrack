use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
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

/// Inserts a workout `mins_ago` minutes back with `sets` sets of a catalog exercise.
async fn seed_workout(pool: &PgPool, user: Uuid, mins_ago: i64, exercise: &str, sets: i32) {
    let wid: Uuid = sqlx::query_scalar(
        "INSERT INTO workouts (user_id, started_at, duration_s) VALUES ($1, $2, 1800) RETURNING id",
    )
    .bind(user)
    .bind(Utc::now() - Duration::minutes(mins_ago))
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

async fn seed_food(pool: &PgPool, user: Uuid, mins_ago: i64, protein: f64) {
    let food: Uuid = sqlx::query_scalar(
        "INSERT INTO foods (name, kcal, protein_g, carbs_g, fat_g, serving_g, source, owner_id) \
         VALUES ('Shake', 200, $2, 10, 2, 100, 'custom', $1) RETURNING id",
    )
    .bind(user)
    .bind(protein)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO food_logs (id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g) \
         VALUES (gen_random_uuid(), $1, $2, $3, 1, 200, $4, 10, 2)",
    )
    .bind(user)
    .bind(food)
    .bind(Utc::now() - Duration::minutes(mins_ago))
    .bind(protein)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_sleep(pool: &PgPool, user: Uuid, woke_mins_ago: i64, hours: i64) {
    let wake: DateTime<Utc> = Utc::now() - Duration::minutes(woke_mins_ago);
    sqlx::query("INSERT INTO sleep_logs (user_id, bedtime, wake_time) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(wake - Duration::hours(hours))
        .bind(wake)
        .execute(pool)
        .await
        .unwrap();
}

async fn progression(app: &Router, tok: &str) -> Value {
    let (s, b) = call(app, Method::GET, "/progression", None, Some(tok)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b
}

fn hints(p: &Value) -> Vec<&str> {
    p["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap())
        .collect()
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    for uri in ["/progression", "/records", "/program"] {
        let (s, _) = call(&app, Method::GET, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[sqlx::test]
async fn flag_off_is_404(pool: PgPool) {
    let mut cfg = Config::for_test();
    cfg.features.clear();
    let app = router_with(pool, cfg);
    let (tok, _) = register(&app, "ann@example.com").await;
    for uri in ["/progression", "/records", "/program"] {
        let (s, _) = call(&app, Method::GET, uri, None, Some(&tok)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test]
async fn new_user_has_open_windows_level_one_and_a_fuel_hint(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;
    let p = progression(&app, &tok).await;
    for k in ["strength", "cardio"] {
        let t = &p["tracks"][k];
        assert_eq!(t["state"], "open", "{p}");
        assert_eq!(t["fraction"], 1.0);
        assert_eq!(t["level"], 1);
        assert_eq!(t["points"], 0);
        assert_eq!(t["progress"], 0.0);
        assert_eq!(t["value"], 0);
        assert!(t["opens_at"].is_null() && t["closes_at"].is_null());
        assert_eq!(t["protein_bonus"], false);
    }
    assert_eq!(p["tracks"]["focus"]["level"], 1);
    assert!(p["tracks"]["focus"].get("state").is_none());
    assert_eq!(
        p["ring"],
        json!({ "minutes": 0, "goal": 30, "closed": false })
    );
    assert_eq!(
        hints(&p),
        ["Eat something before training for more energy."]
    );
    assert_eq!(p["new_personal_bests"], json!([]));
}

#[sqlx::test]
async fn lifting_starts_recovery_scores_a_point_and_closes_the_ring(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    // A bare workout (no sets) counts for cardio; the squat session counts for strength.
    sqlx::query(
        "INSERT INTO workouts (user_id, started_at, duration_s) \
         VALUES ($1, greatest(now() - interval '1 minute', date_trunc('day', now())), 2400)",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    seed_workout(&pool, uid, 1, "Squat", 3).await;
    let p = progression(&app, &tok).await;
    let s = &p["tracks"]["strength"];
    assert_eq!(s["state"], "recovering", "{p}");
    assert_eq!(s["points"], 1);
    assert!(s["opens_at"].is_string());
    assert!(s["fraction"].as_f64().unwrap() < 0.05);
    assert_eq!(p["tracks"]["cardio"]["state"], "recovering");
    assert_eq!(p["tracks"]["cardio"]["points"], 1);
    assert_eq!(p["tracks"]["focus"]["points"], 0);
    // 40 minutes today closes the ring.
    assert_eq!(p["ring"]["closed"], true);
    let h = hints(&p);
    assert!(
        h.contains(&"Done for today \u{2014} extra training won't add more."),
        "{h:?}"
    );
    assert!(h.contains(&"Workout complete \u{2014} time to rest and recover."));
    assert!(h.contains(&"Have ~25 g protein in the next 2 hours to recover twice as fast."));
}

#[sqlx::test]
async fn protein_after_the_session_halves_recovery(pool: PgPool) {
    let app = router(pool.clone());
    let (ann, ann_id) = register(&app, "ann@example.com").await;
    let (bob, bob_id) = register(&app, "bob@example.com").await;
    // Both lifted 11 hours ago; only Ann had 30 g protein half an hour later.
    for u in [ann_id, bob_id] {
        seed_workout(&pool, u, 11 * 60, "Squat", 3).await;
    }
    seed_food(&pool, ann_id, 10 * 60 + 30, 30.0).await;
    let a = progression(&app, &ann).await;
    assert_eq!(a["tracks"]["strength"]["state"], "open", "{a}");
    assert_eq!(a["tracks"]["strength"]["protein_bonus"], true);
    let b = progression(&app, &bob).await;
    assert_eq!(b["tracks"]["strength"]["state"], "recovering");
    assert_eq!(b["tracks"]["strength"]["protein_bonus"], false);
}

#[sqlx::test]
async fn a_night_of_sleep_is_required_to_open_the_window(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    seed_workout(&pool, uid, 30 * 60, "Squat", 3).await;
    // Only a 3 hour sleep since the workout: the body still needs rest.
    seed_sleep(&pool, uid, 20 * 60, 3).await;
    let p = progression(&app, &tok).await;
    assert_eq!(p["tracks"]["strength"]["state"], "needs_rest", "{p}");
    assert!(p["tracks"]["strength"]["opens_at"].is_null());
    // A proper night ends the requirement.
    seed_sleep(&pool, uid, 5 * 60, 7).await;
    let p = progression(&app, &tok).await;
    assert_eq!(p["tracks"]["strength"]["state"], "open", "{p}");
    assert!(p["tracks"]["strength"]["fraction"].as_f64().unwrap() > 0.5);
}

#[sqlx::test]
async fn an_unused_window_is_missed_and_costs_the_point(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    // Last lift 5 days ago: the window (20h to 68h after it) closed long ago.
    seed_workout(&pool, uid, 5 * 24 * 60, "Squat", 3).await;
    let p = progression(&app, &tok).await;
    let s = &p["tracks"]["strength"];
    assert_eq!(s["state"], "missed", "{p}");
    assert_eq!(s["points"], 0);
    assert_eq!(s["fraction"], 0.0);
}

#[sqlx::test]
async fn focus_activities_train_focus_only_and_never_lose_points(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    seed_workout(&pool, uid, 10, "Meditation", 1).await;
    seed_workout(&pool, uid, 20, "Yoga", 1).await; // same day as the first, or the day before
    seed_workout(&pool, uid, 3 * 24 * 60, "Meditation", 1).await;
    let p = progression(&app, &tok).await;
    let points = p["tracks"]["focus"]["points"].as_i64().unwrap();
    assert!((2..=3).contains(&points), "{p}");
    assert_eq!(p["tracks"]["strength"]["points"], 0);
    assert_eq!(p["tracks"]["cardio"]["points"], 0);
    // An old session is still a point: focus only gains.
    seed_workout(&pool, uid, 200 * 24 * 60, "Meditation", 1).await;
    let later = progression(&app, &tok).await;
    assert_eq!(
        later["tracks"]["focus"]["points"].as_i64().unwrap(),
        points + 1
    );
}

#[sqlx::test]
async fn progression_is_isolated_per_user(pool: PgPool) {
    let app = router(pool.clone());
    let (_, ann_id) = register(&app, "ann@example.com").await;
    let (bob, _) = register(&app, "bob@example.com").await;
    seed_workout(&pool, ann_id, 60, "Squat", 3).await;
    seed_food(&pool, ann_id, 30, 40.0).await;
    let b = progression(&app, &bob).await;
    assert_eq!(b["tracks"]["strength"]["points"], 0);
    assert_eq!(b["tracks"]["strength"]["state"], "open");
    assert_eq!(b["tracks"]["strength"]["protein_bonus"], false);
}

#[sqlx::test]
async fn deleted_workouts_are_ignored(pool: PgPool) {
    let app = router(pool.clone());
    let (tok, uid) = register(&app, "ann@example.com").await;
    seed_workout(&pool, uid, 60, "Squat", 3).await;
    sqlx::query("UPDATE workouts SET deleted_at = now() WHERE user_id = $1")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    let p = progression(&app, &tok).await;
    assert_eq!(p["tracks"]["strength"]["points"], 0);
    assert_eq!(p["tracks"]["strength"]["state"], "open");
}

#[sqlx::test]
async fn focus_exercises_are_in_the_catalog_and_can_be_created(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;
    let (_, all) = call(&app, Method::GET, "/exercises?q=a", None, Some(&tok)).await;
    let focus = all
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["muscle_group"] == "focus")
        .count();
    assert!(focus >= 5, "{all}");
    let (s, e) = call(
        &app,
        Method::POST,
        "/exercises",
        Some(json!({ "name": "Archery", "muscle_group": "focus" })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{e}");
}

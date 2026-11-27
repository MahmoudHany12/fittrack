use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use fittrack_api::{app, config::Config, sleep, AppState};
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

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

/// A sleep body ending `ended_ago` hours before now and lasting `hours`.
fn body(ended_ago: i64, hours: i64) -> Value {
    let wake = Utc::now() - Duration::hours(ended_ago);
    json!({ "bedtime": ts(wake - Duration::hours(hours)), "wake_time": ts(wake) })
}

#[sqlx::test]
async fn crud_roundtrip(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;

    let mut b = body(2, 8);
    b["quality"] = json!(4);
    let (s, created) = call(&app, Method::POST, "/sleep", Some(b), Some(&tok)).await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    assert_eq!(created["duration_minutes"], 480);
    assert_eq!(created["quality"], 4);
    assert_eq!(created["source"], "manual");
    let id = created["id"].as_str().unwrap().to_string();

    let (s, list) = call(&app, Method::GET, "/sleep", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());

    // update: 7 h 30 min, quality cleared
    let wake = Utc::now() - Duration::hours(2);
    let upd = json!({
        "bedtime": ts(wake - Duration::minutes(450)),
        "wake_time": ts(wake),
        "quality": null,
    });
    let (s, updated) = call(
        &app,
        Method::PUT,
        &format!("/sleep/{id}"),
        Some(upd),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{updated}");
    assert_eq!(updated["duration_minutes"], 450);
    assert!(updated["quality"].is_null());

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/sleep/{id}"),
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&tok)).await;
    assert!(list.as_array().unwrap().is_empty());

    // deleted rows are gone for update and delete as well
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/sleep/{id}"),
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/sleep/{id}"),
        Some(body(2, 8)),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn requires_auth(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/sleep", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, Method::POST, "/sleep", Some(body(1, 8)), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn validation_rules(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;
    let now = Utc::now();

    // wake before bedtime
    let (s, b) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(json!({ "bedtime": ts(now), "wake_time": ts(now - Duration::hours(1)) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["error"]["fields"]["wake_time"].is_array());

    // equal times
    let (s, _) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(json!({ "bedtime": ts(now), "wake_time": ts(now) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // longer than 24 h
    let (s, _) = call(&app, Method::POST, "/sleep", Some(body(1, 25)), Some(&tok)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // more than a day in the future
    let (s, _) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(json!({
            "bedtime": ts(now + Duration::hours(40)),
            "wake_time": ts(now + Duration::hours(48)),
        })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // quality out of range, both ends
    for q in [0, 6, -1] {
        let mut b = body(1, 8);
        b["quality"] = json!(q);
        let (s, r) = call(&app, Method::POST, "/sleep", Some(b), Some(&tok)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "quality {q}");
        assert!(r["error"]["fields"]["quality"].is_array());
    }

    // bad source, missing field, bad timestamp
    let mut b = body(1, 8);
    b["source"] = json!("telepathy");
    let (s, _) = call(&app, Method::POST, "/sleep", Some(b), Some(&tok)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(json!({ "bedtime": ts(now) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(json!({ "bedtime": "yesterday", "wake_time": "today" })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // PUT validates too
    let (_, created) = call(&app, Method::POST, "/sleep", Some(body(1, 8)), Some(&tok)).await;
    let id = created["id"].as_str().unwrap();
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/sleep/{id}"),
        Some(json!({ "bedtime": ts(now), "wake_time": ts(now - Duration::hours(2)) })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // exactly 24 h is allowed
    let (s, _) = call(&app, Method::POST, "/sleep", Some(body(30, 24)), Some(&tok)).await;
    assert_eq!(s, StatusCode::CREATED);
}

#[sqlx::test]
async fn list_is_newest_first_and_filters_by_range(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;
    for ended_ago in [60, 12, 36] {
        let (s, _) = call(
            &app,
            Method::POST,
            "/sleep",
            Some(body(ended_ago, 7)),
            Some(&tok),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&tok)).await;
    let wakes: Vec<DateTime<Utc>> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["wake_time"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(wakes.len(), 3);
    assert!(wakes[0] > wakes[1] && wakes[1] > wakes[2]);

    let from = Utc::now() - Duration::hours(48);
    let to = Utc::now() - Duration::hours(24);
    let uri = format!(
        "/sleep?from={}&to={}",
        ts(from).replace('+', "%2B"),
        ts(to).replace('+', "%2B")
    );
    let (s, list) = call(&app, Method::GET, &uri, None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn cross_user_isolation(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;

    let (_, created) = call(&app, Method::POST, "/sleep", Some(body(2, 8)), Some(&a)).await;
    let id = created["id"].as_str().unwrap().to_string();

    // B sees nothing
    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&b)).await;
    assert!(list.as_array().unwrap().is_empty());

    // B cannot update or delete A's row
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/sleep/{id}"),
        Some(body(2, 6)),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/sleep/{id}"),
        None,
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // B cannot overwrite A's id via POST
    let mut hijack = body(2, 3);
    hijack["id"] = json!(id);
    let (s, _) = call(&app, Method::POST, "/sleep", Some(hijack), Some(&b)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // A's row is untouched
    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&a)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["duration_minutes"], 480);
}

#[sqlx::test]
async fn post_with_same_id_is_idempotent_upsert(pool: PgPool) {
    let app = router(pool);
    let (tok, _) = register(&app, "ann@example.com").await;
    let id = Uuid::new_v4();

    let mut b = body(2, 8);
    b["id"] = json!(id);
    let (s, first) = call(&app, Method::POST, "/sleep", Some(b.clone()), Some(&tok)).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(first["id"], id.to_string());

    // exact retry: still one row
    let (s, _) = call(&app, Method::POST, "/sleep", Some(b), Some(&tok)).await;
    assert_eq!(s, StatusCode::OK);

    // same id with new data updates it
    let mut b2 = body(2, 6);
    b2["id"] = json!(id);
    b2["quality"] = json!(5);
    let (s, upd) = call(&app, Method::POST, "/sleep", Some(b2), Some(&tok)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(upd["duration_minutes"], 360);
    assert_eq!(upd["quality"], 5);

    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&tok)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn external_id_is_unique_per_user_and_source(pool: PgPool) {
    let app = router(pool);
    let (a, _) = register(&app, "ann@example.com").await;
    let (b, _) = register(&app, "bob@example.com").await;

    let mk = |ext: &str| {
        let mut v = body(2, 8);
        v["source"] = json!("health");
        v["external_id"] = json!(ext);
        v
    };
    let (s, _) = call(&app, Method::POST, "/sleep", Some(mk("hk-1")), Some(&a)).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, r) = call(&app, Method::POST, "/sleep", Some(mk("hk-1")), Some(&a)).await;
    assert_eq!(s, StatusCode::CONFLICT, "{r}");
    // another user may use the same external id
    let (s, _) = call(&app, Method::POST, "/sleep", Some(mk("hk-1")), Some(&b)).await;
    assert_eq!(s, StatusCode::CREATED);
}

#[sqlx::test]
async fn last_night_picks_the_right_row(pool: PgPool) {
    let app = router(pool.clone());
    let (a, a_id) = register(&app, "ann@example.com").await;
    let (b, b_id) = register(&app, "bob@example.com").await;
    let now = Utc::now();

    assert_eq!(sleep::last_night(&pool, a_id, now).await.unwrap(), None);

    // too old (woke 40 h ago), a nap this morning, last night's sleep (woke 5 h ago)
    let mk = |ended_ago_h: i64, hours: i64, q: Option<i16>| {
        let mut v = body(ended_ago_h, hours);
        if let Some(q) = q {
            v["quality"] = json!(q);
        }
        v
    };
    for v in [mk(40, 8, Some(1)), mk(5, 8, Some(4)), mk(1, 1, None)] {
        let (s, _) = call(&app, Method::POST, "/sleep", Some(v), Some(&a)).await;
        assert_eq!(s, StatusCode::CREATED);
    }
    // another user's later sleep must not leak
    let (s, _) = call(
        &app,
        Method::POST,
        "/sleep",
        Some(mk(0, 9, Some(5))),
        Some(&b),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // most recent wake_time wins (the nap)
    let got = sleep::last_night(&pool, a_id, now).await.unwrap().unwrap();
    assert_eq!(got.minutes, 60);
    assert_eq!(got.quality, None);

    // soft-deleting the nap exposes last night's sleep
    let (_, list) = call(&app, Method::GET, "/sleep", None, Some(&a)).await;
    let nap_id = list[0]["id"].as_str().unwrap().to_string();
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/sleep/{nap_id}"),
        None,
        Some(&a),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let got = sleep::last_night(&pool, a_id, now).await.unwrap().unwrap();
    assert_eq!(got.minutes, 480);
    assert_eq!(got.quality, Some(4));

    // outside the 36 h window nothing is returned
    let later = now + Duration::hours(48);
    assert_eq!(sleep::last_night(&pool, a_id, later).await.unwrap(), None);

    // bob has his own
    let got = sleep::last_night(&pool, b_id, now + Duration::hours(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.minutes, 540);
}

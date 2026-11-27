//! Chat: friends-only access, privacy, validation, rate limit, read marking, pagination.
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

struct User {
    id: String,
    token: String,
}

impl User {
    fn uuid(&self) -> Uuid {
        Uuid::parse_str(&self.id).unwrap()
    }
}

async fn register(app: &Router, email: &str, name: &str) -> User {
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
    let (s, b) = call(
        app,
        Method::PUT,
        "/profile",
        Some(json!({ "display_name": name, "calorie_target": 2200, "units": "metric" })),
        Some(&token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    User {
        id: me["id"].as_str().unwrap().to_string(),
        token,
    }
}

async fn befriend(app: &Router, a: &User, b: &User) {
    let (s, inv) = call(app, Method::POST, "/friends/invites", None, Some(&a.token)).await;
    assert_eq!(s, StatusCode::CREATED, "{inv}");
    let code = inv["code"].as_str().unwrap();
    let (s, body) = call(
        app,
        Method::POST,
        &format!("/friends/invites/{code}/accept"),
        None,
        Some(&b.token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

async fn two_friends(app: &Router) -> (User, User) {
    let ann = register(app, "ann@example.com", "Ann").await;
    let bob = register(app, "bob@example.com", "Bob").await;
    befriend(app, &ann, &bob).await;
    (ann, bob)
}

async fn send(app: &Router, from: &User, to: &User, body: &str) -> (StatusCode, Value) {
    call(
        app,
        Method::POST,
        &format!("/chat/{}", to.id),
        Some(json!({ "body": body })),
        Some(&from.token),
    )
    .await
}

async fn thread(app: &Router, me: &User, other: &User, query: &str) -> (StatusCode, Value) {
    call(
        app,
        Method::GET,
        &format!("/chat/{}{query}", other.id),
        None,
        Some(&me.token),
    )
    .await
}

#[sqlx::test]
async fn friends_can_exchange_messages(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    let (s, m) = send(&app, &ann, &bob, "  hello bob  ").await;
    assert_eq!(s, StatusCode::CREATED, "{m}");
    assert_eq!(m["body"], "hello bob");
    assert_eq!(m["mine"], true);
    let (s, _) = send(&app, &bob, &ann, "hi ann").await;
    assert_eq!(s, StatusCode::CREATED);

    let (s, t) = thread(&app, &ann, &bob, "").await;
    assert_eq!(s, StatusCode::OK);
    let msgs = t.as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    // newest first
    assert_eq!(msgs[0]["body"], "hi ann");
    assert_eq!(msgs[0]["mine"], false);
    assert_eq!(msgs[1]["body"], "hello bob");
    assert_eq!(msgs[1]["mine"], true);
}

#[sqlx::test]
async fn non_friends_get_404_on_read_and_send(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let cat = register(&app, "cat@example.com", "Cat").await;
    let (s, _) = send(&app, &ann, &cat, "hey").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = thread(&app, &ann, &cat, "").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/chat/{}/messages/{}", cat.id, Uuid::new_v4()),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn third_party_cannot_read_or_delete(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    let cat = register(&app, "cat@example.com", "Cat").await;
    befriend(&app, &cat, &ann).await;
    befriend(&app, &cat, &bob).await;
    let (_, m) = send(&app, &ann, &bob, "secret").await;
    let id = m["id"].as_str().unwrap();

    // Cat is a friend of both but is not part of their conversation.
    let (s, t) = thread(&app, &cat, &ann, "").await;
    assert_eq!(s, StatusCode::OK);
    assert!(t.as_array().unwrap().is_empty());
    let (s, t) = call(&app, Method::GET, "/chat", None, Some(&cat.token)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(t.as_array().unwrap().is_empty());
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/chat/{}/messages/{id}", ann.id),
        None,
        Some(&cat.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, t) = thread(&app, &ann, &bob, "").await;
    assert_eq!(t.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn unfriending_blocks_access_and_hides_history(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    send(&app, &ann, &bob, "before").await;
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/friends/{}", bob.id),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    for (a, b) in [(&ann, &bob), (&bob, &ann)] {
        let (s, _) = thread(&app, a, b, "").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = send(&app, a, b, "after").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, list) = call(&app, Method::GET, "/chat", None, Some(&a.token)).await;
        assert_eq!(s, StatusCode::OK);
        assert!(list.as_array().unwrap().is_empty());
    }
}

#[sqlx::test]
async fn only_sender_can_delete(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    let (_, m) = send(&app, &ann, &bob, "oops").await;
    let id = m["id"].as_str().unwrap();

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/chat/{}/messages/{id}", ann.id),
        None,
        Some(&bob.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, t) = thread(&app, &bob, &ann, "").await;
    assert_eq!(t.as_array().unwrap().len(), 1);

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/chat/{}/messages/{id}", bob.id),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, t) = thread(&app, &bob, &ann, "").await;
    assert!(t.as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn reading_marks_received_messages_read(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    send(&app, &ann, &bob, "one").await;
    send(&app, &ann, &bob, "two").await;

    let (_, list) = call(&app, Method::GET, "/chat", None, Some(&bob.token)).await;
    assert_eq!(list[0]["unread_count"], 2);

    // The sender reading their own thread does not mark anything read.
    let (_, t) = thread(&app, &ann, &bob, "").await;
    assert!(t[0]["read_at"].is_null());
    let (_, list) = call(&app, Method::GET, "/chat", None, Some(&bob.token)).await;
    assert_eq!(list[0]["unread_count"], 2);

    let (_, t) = thread(&app, &bob, &ann, "").await;
    assert!(t[0]["read_at"].is_string());
    let (_, list) = call(&app, Method::GET, "/chat", None, Some(&bob.token)).await;
    assert_eq!(list[0]["unread_count"], 0);
    // The sender sees them as read now.
    let (_, t) = thread(&app, &ann, &bob, "").await;
    assert!(t[0]["read_at"].is_string());
}

#[sqlx::test]
async fn pagination_with_before_and_limit(pool: PgPool) {
    let app = router(pool.clone());
    let (ann, bob) = two_friends(&app).await;
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO messages (from_user, to_user, body, sent_at)
             VALUES ($1, $2, $3, now() - make_interval(mins => $4))",
        )
        .bind(ann.uuid())
        .bind(bob.uuid())
        .bind(format!("m{i}"))
        .bind(10 - i)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (_, page1) = thread(&app, &bob, &ann, "?limit=2").await;
    let p1: Vec<&str> = page1
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(p1, ["m4", "m3"]);
    let before = page1[1]["sent_at"].as_str().unwrap().replace('+', "%2B");
    let (s, page2) = thread(&app, &bob, &ann, &format!("?limit=2&before={before}")).await;
    assert_eq!(s, StatusCode::OK);
    let p2: Vec<&str> = page2
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(p2, ["m2", "m1"]);
    // Oversized limits are capped, not rejected.
    let (s, all) = thread(&app, &bob, &ann, "?limit=100000").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 5);
}

#[sqlx::test]
async fn body_validation(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    for bad in [
        String::new(),
        "   \n ".to_string(),
        "a".repeat(2001),
        "bell\u{0007}".to_string(),
        "nul\u{0000}x".to_string(),
        "tab\there".to_string(),
    ] {
        let (s, b) = send(&app, &ann, &bob, &bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
    let (s, _) = send(&app, &ann, &bob, &"a".repeat(2000)).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = send(&app, &ann, &bob, "line one\nline two").await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = call(
        &app,
        Method::POST,
        &format!("/chat/{}", bob.id),
        Some(json!({})),
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn rate_limit_30_per_minute(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    for i in 0..30 {
        let (s, b) = send(&app, &ann, &bob, &format!("m{i}")).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }
    let (s, b) = send(&app, &ann, &bob, "one too many").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b["error"]["message"]
        .as_str()
        .unwrap()
        .contains("per minute"));
    // The limit is per sender.
    let (s, _) = send(&app, &bob, &ann, "still fine").await;
    assert_eq!(s, StatusCode::CREATED);
}

#[sqlx::test]
async fn flag_off_returns_404_even_without_auth(pool: PgPool) {
    let mut config = Config::for_test();
    config.features.remove("chat");
    let app = router_with(pool, config);
    let id = Uuid::new_v4();
    for (m, uri, body) in [
        (Method::GET, "/chat".to_string(), None),
        (Method::GET, format!("/chat/{id}"), None),
        (
            Method::POST,
            format!("/chat/{id}"),
            Some(json!({ "body": "x" })),
        ),
        (
            Method::DELETE,
            format!("/chat/{id}/messages/{}", Uuid::new_v4()),
            None,
        ),
    ] {
        let (s, _) = call(&app, m, &uri, body, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test]
async fn chat_requires_auth_when_enabled(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::GET, "/chat", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn conversation_list_unread_counts_preview_and_order(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let bob = register(&app, "bob@example.com", "Bob").await;
    let cat = register(&app, "cat@example.com", "Cat").await;
    befriend(&app, &ann, &bob).await;
    befriend(&app, &ann, &cat).await;

    send(&app, &bob, &ann, "from bob 1").await;
    send(&app, &bob, &ann, "from bob 2").await;
    let long = "x".repeat(200);
    send(&app, &cat, &ann, &long).await;

    let (s, list) = call(&app, Method::GET, "/chat", None, Some(&ann.token)).await;
    assert_eq!(s, StatusCode::OK);
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // newest conversation first (Cat wrote last)
    assert_eq!(rows[0]["display_name"], "Cat");
    assert_eq!(rows[0]["unread_count"], 1);
    assert_eq!(
        rows[0]["last_message"].as_str().unwrap().chars().count(),
        80
    );
    assert_eq!(rows[1]["display_name"], "Bob");
    assert_eq!(rows[1]["unread_count"], 2);
    assert_eq!(rows[1]["last_message"], "from bob 2");
    assert_eq!(rows[1]["friend_id"], bob.id);

    // Reading Bob's thread only clears Bob's count.
    thread(&app, &ann, &bob, "").await;
    let (_, list) = call(&app, Method::GET, "/chat", None, Some(&ann.token)).await;
    assert_eq!(list[0]["unread_count"], 1);
    assert_eq!(list[1]["unread_count"], 0);
}

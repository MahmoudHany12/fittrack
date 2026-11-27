//! Friends: invite lifecycle, accept rules and (required) sharing/privacy tests.
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

async fn invite(app: &Router, owner: &User) -> String {
    let (s, b) = call(
        app,
        Method::POST,
        "/friends/invites",
        None,
        Some(&owner.token),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["code"].as_str().unwrap().to_string()
}

async fn accept(app: &Router, who: &User, code: &str) -> (StatusCode, Value) {
    call(
        app,
        Method::POST,
        &format!("/friends/invites/{code}/accept"),
        None,
        Some(&who.token),
    )
    .await
}

async fn befriend(app: &Router, a: &User, b: &User) {
    let code = invite(app, a).await;
    let (s, body) = accept(app, b, &code).await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

async fn share(app: &Router, owner: &User, viewer: &User, enabled: bool) -> (StatusCode, Value) {
    call(
        app,
        Method::PUT,
        &format!("/friends/{}/share", viewer.id),
        Some(json!({ "enabled": enabled })),
        Some(&owner.token),
    )
    .await
}

async fn summary(app: &Router, viewer: &User, owner: &User) -> (StatusCode, Value) {
    call(
        app,
        Method::GET,
        &format!("/friends/{}/summary", owner.id),
        None,
        Some(&viewer.token),
    )
    .await
}

async fn two_friends(app: &Router) -> (User, User) {
    let ann = register(app, "ann@example.com", "Ann").await;
    let bob = register(app, "bob@example.com", "Bob").await;
    befriend(app, &ann, &bob).await;
    (ann, bob)
}

#[sqlx::test]
async fn invite_has_code_link_and_expiry(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let (s, b) = call(
        &app,
        Method::POST,
        "/friends/invites",
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let code = b["code"].as_str().unwrap();
    assert_eq!(code.len(), 10);
    assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    assert!(!code.contains(['0', 'O', '1', 'I']));
    assert_eq!(b["link"], format!("fittrack://invite/{code}"));
    assert!(b["expires_at"].is_string());
}

#[sqlx::test]
async fn invites_require_auth(pool: PgPool) {
    let app = router(pool);
    let (s, _) = call(&app, Method::POST, "/friends/invites", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, Method::GET, "/friends", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn live_invites_are_capped_at_20(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    for _ in 0..20 {
        invite(&app, &ann).await;
    }
    let (s, b) = call(
        &app,
        Method::POST,
        "/friends/invites",
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
}

#[sqlx::test]
async fn accept_makes_friends_and_invite_is_single_use(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let bob = register(&app, "bob@example.com", "Bob").await;
    let cat = register(&app, "cat@example.com", "Cat").await;
    let code = invite(&app, &ann).await;

    let (s, b) = accept(&app, &bob, &code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["friend_id"], ann.id);

    // Used code now looks exactly like an unknown one.
    let (s_used, b_used) = accept(&app, &cat, &code).await;
    let (s_unknown, b_unknown) = accept(&app, &cat, "ZZZZZZZZZZ").await;
    assert_eq!(s_used, StatusCode::NOT_FOUND);
    assert_eq!(s_unknown, StatusCode::NOT_FOUND);
    assert_eq!(b_used, b_unknown);

    // Both sides see each other, shown by display name only.
    let (_, list) = call(&app, Method::GET, "/friends", None, Some(&ann.token)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["user_id"], bob.id);
    assert_eq!(list[0]["display_name"], "Bob");
    assert_eq!(list[0]["sharing_with_me"], false);
    assert_eq!(list[0]["i_share"], false);
    assert!(list[0].get("email").is_none());
    let (_, list) = call(&app, Method::GET, "/friends", None, Some(&bob.token)).await;
    assert_eq!(list[0]["user_id"], ann.id);
}

#[sqlx::test]
async fn cannot_accept_own_invite(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    let code = invite(&app, &ann).await;
    let (s, _) = accept(&app, &ann, &code).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // The invite was not consumed.
    let bob = register(&app, "bob@example.com", "Bob").await;
    let (s, _) = accept(&app, &bob, &code).await;
    assert_eq!(s, StatusCode::OK);
}

#[sqlx::test]
async fn expired_invite_is_404(pool: PgPool) {
    let app = router(pool.clone());
    let ann = register(&app, "ann@example.com", "Ann").await;
    let bob = register(&app, "bob@example.com", "Bob").await;
    let code = invite(&app, &ann).await;
    sqlx::query("UPDATE invites SET expires_at = now() - interval '1 minute' WHERE code = $1")
        .bind(&code)
        .execute(&pool)
        .await
        .unwrap();
    let (s, _) = accept(&app, &bob, &code).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn expired_invites_do_not_count_toward_the_cap(pool: PgPool) {
    let app = router(pool.clone());
    let ann = register(&app, "ann@example.com", "Ann").await;
    for _ in 0..20 {
        invite(&app, &ann).await;
    }
    sqlx::query("UPDATE invites SET expires_at = now() - interval '1 day'")
        .execute(&pool)
        .await
        .unwrap();
    invite(&app, &ann).await;
}

#[sqlx::test]
async fn already_friends_is_409(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    // Either direction.
    let code = invite(&app, &ann).await;
    let (s, _) = accept(&app, &bob, &code).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let code = invite(&app, &bob).await;
    let (s, _) = accept(&app, &ann, &code).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn malformed_codes_are_404(pool: PgPool) {
    let app = router(pool);
    let ann = register(&app, "ann@example.com", "Ann").await;
    for code in ["short", "0OIl1AAAAA", "ABCDEFGHJKLMN"] {
        let (s, _) = accept(&app, &ann, code).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{code}");
    }
}

#[sqlx::test]
async fn sharing_is_off_by_default_then_grant_then_revoke(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;

    // Default off -> 403 in both directions.
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::FORBIDDEN);
    assert_eq!(summary(&app, &ann, &bob).await.0, StatusCode::FORBIDDEN);

    // Ann grants Bob: Bob can read, Ann still cannot read Bob's (one-way).
    let (s, b) = share(&app, &ann, &bob, true).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["enabled"], true);
    let (s, body) = summary(&app, &bob, &ann).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["user_id"], ann.id);
    assert_eq!(body["display_name"], "Ann");
    assert!(body.get("email").is_none());
    assert_eq!(summary(&app, &ann, &bob).await.0, StatusCode::FORBIDDEN);

    // Flags in the list are per viewpoint.
    let (_, list) = call(&app, Method::GET, "/friends", None, Some(&bob.token)).await;
    assert_eq!(list[0]["sharing_with_me"], true);
    assert_eq!(list[0]["i_share"], false);
    let (_, list) = call(&app, Method::GET, "/friends", None, Some(&ann.token)).await;
    assert_eq!(list[0]["sharing_with_me"], false);
    assert_eq!(list[0]["i_share"], true);

    // Revoke takes effect immediately.
    let (s, _) = share(&app, &ann, &bob, false).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::FORBIDDEN);

    // Revoking again is harmless; re-granting works.
    assert_eq!(share(&app, &ann, &bob, false).await.0, StatusCode::OK);
    assert_eq!(share(&app, &ann, &bob, true).await.0, StatusCode::OK);
    assert_eq!(share(&app, &ann, &bob, true).await.0, StatusCode::OK);
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn unfriend_removes_friendship_and_revokes_both_grants(pool: PgPool) {
    let app = router(pool.clone());
    let (ann, bob) = two_friends(&app).await;
    share(&app, &ann, &bob, true).await;
    share(&app, &bob, &ann, true).await;
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::OK);
    assert_eq!(summary(&app, &ann, &bob).await.0, StatusCode::OK);

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/friends/{}", bob.id),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // No longer friends -> 404 both ways.
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::NOT_FOUND);
    assert_eq!(summary(&app, &ann, &bob).await.0, StatusCode::NOT_FOUND);
    let (_, list) = call(&app, Method::GET, "/friends", None, Some(&ann.token)).await;
    assert!(list.as_array().unwrap().is_empty());

    // Grants are revoked in the database, so re-friending does not resurrect them.
    let active: i64 =
        sqlx::query_scalar("SELECT count(*) FROM share_grants WHERE revoked_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(active, 0);
    befriend(&app, &bob, &ann).await;
    assert_eq!(summary(&app, &bob, &ann).await.0, StatusCode::FORBIDDEN);
    assert_eq!(summary(&app, &ann, &bob).await.0, StatusCode::FORBIDDEN);

    // Unfriending a non-friend is 404.
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/friends/{}", uuid_like()),
        None,
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

fn uuid_like() -> String {
    "00000000-0000-4000-8000-000000000001".into()
}

#[sqlx::test]
async fn non_friend_summary_is_404_and_stranger_cannot_read(pool: PgPool) {
    let app = router(pool);
    let (ann, _bob) = two_friends(&app).await;
    let eve = register(&app, "eve@example.com", "Eve").await;
    // Even once Ann shares with Bob, Eve (a stranger) gets 404, never data.
    let bob_id = _bob.id.clone();
    assert_eq!(share(&app, &ann, &_bob, true).await.0, StatusCode::OK);
    let (s, body) = summary(&app, &eve, &ann).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(body.get("summary").is_none());
    // Unknown user id behaves the same as an existing non-friend.
    let (s, _) = call(
        &app,
        Method::GET,
        &format!("/friends/{}/summary", uuid_like()),
        None,
        Some(&eve.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // And Eve cannot read Bob's either.
    let (s, _) = call(
        &app,
        Method::GET,
        &format!("/friends/{bob_id}/summary"),
        None,
        Some(&eve.token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Unauthenticated -> 401.
    let (s, _) = call(
        &app,
        Method::GET,
        &format!("/friends/{}/summary", ann.id),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn cannot_grant_to_non_friend_or_self(pool: PgPool) {
    let app = router(pool.clone());
    let ann = register(&app, "ann@example.com", "Ann").await;
    let eve = register(&app, "eve@example.com", "Eve").await;
    let (s, _) = share(&app, &ann, &eve, true).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = share(&app, &ann, &ann, true).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM share_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(grants, 0);
    // Eve still cannot read Ann's summary.
    assert_eq!(summary(&app, &eve, &ann).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn share_validates_body(pool: PgPool) {
    let app = router(pool);
    let (ann, bob) = two_friends(&app).await;
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/friends/{}/share", bob.id),
        Some(json!({ "enabled": "yes" })),
        Some(&ann.token),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        Method::PUT,
        "/friends/not-a-uuid/share",
        Some(json!({ "enabled": true })),
        Some(&ann.token),
    )
    .await;
    assert!(s.is_client_error());
}

#[sqlx::test]
async fn flag_off_means_404_everywhere(pool: PgPool) {
    let mut config = Config::for_test();
    config.features.clear();
    let off = router_with(pool.clone(), config);
    let ann = register(&off, "ann@example.com", "Ann").await;
    let id = uuid_like();
    let routes: Vec<(Method, String, Option<Value>)> = vec![
        (Method::POST, "/friends/invites".into(), None),
        (
            Method::POST,
            "/friends/invites/ABCDEFGHJK/accept".into(),
            None,
        ),
        (Method::GET, "/friends".into(), None),
        (Method::DELETE, format!("/friends/{id}"), None),
        (
            Method::PUT,
            format!("/friends/{id}/share"),
            Some(json!({ "enabled": true })),
        ),
        (Method::GET, format!("/friends/{id}/summary"), None),
    ];
    for (method, uri, body) in routes {
        let (s, _) = call(&off, method.clone(), &uri, body.clone(), Some(&ann.token)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

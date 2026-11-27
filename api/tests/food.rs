use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Extension, Router,
};
use fittrack_api::{
    app,
    config::Config,
    food::providers::{ExternalFood, FoodProvider, FoodProviders},
    AppState,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;
use uuid::Uuid;

struct FakeProvider {
    foods: Vec<ExternalFood>,
    calls: AtomicUsize,
}

#[async_trait]
impl FoodProvider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }
    async fn search(&self, _query: &str) -> anyhow::Result<Vec<ExternalFood>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.foods.clone())
    }
}

struct FailingProvider;

#[async_trait]
impl FoodProvider for FailingProvider {
    fn name(&self) -> &'static str {
        "failing"
    }
    async fn search(&self, _query: &str) -> anyhow::Result<Vec<ExternalFood>> {
        anyhow::bail!("upstream is down")
    }
}

fn oat(kcal: f64) -> ExternalFood {
    ExternalFood {
        source: "off".into(),
        external_id: "5000001".into(),
        name: "Oat Granola".into(),
        brand: Some("Acme".into()),
        kcal,
        protein_g: 4.0,
        carbs_g: 30.0,
        fat_g: 6.0,
        serving_g: 50.0,
    }
}

fn fake(foods: Vec<ExternalFood>) -> Arc<FakeProvider> {
    Arc::new(FakeProvider {
        foods,
        calls: AtomicUsize::new(0),
    })
}

fn router_with(pool: PgPool, providers: Vec<Arc<dyn FoodProvider>>) -> Router {
    app(AppState {
        pool,
        config: Arc::new(Config::for_test()),
    })
    .layer(Extension(FoodProviders::new(providers)))
}

fn router(pool: PgPool) -> Router {
    router_with(pool, vec![fake(vec![])])
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

async fn token(app: &Router, email: &str) -> String {
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

async fn custom_food(app: &Router, t: &str, name: &str, kcal: f64) -> Value {
    let (status, body) = call(
        app,
        Method::POST,
        "/foods",
        Some(json!({
            "name": name, "kcal": kcal, "protein_g": 10.0, "carbs_g": 20.0,
            "fat_g": 5.0, "serving_g": 100.0
        })),
        Some(t),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

fn approx(v: &Value, expected: f64) {
    let got = v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"));
    assert!(
        (got - expected).abs() < 0.01,
        "got {got}, expected {expected}"
    );
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[sqlx::test]
async fn custom_food_log_and_summary_math(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    let food = custom_food(&app, &t, "Chicken bowl", 250.0).await;
    assert_eq!(food["source"], "custom");
    let food_id = food["id"].as_str().unwrap();

    let (s, log) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({ "food_id": food_id, "servings": 1.5, "meal": "lunch" })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{log}");
    approx(&log["kcal"], 375.0);
    approx(&log["protein_g"], 15.0);
    approx(&log["carbs_g"], 30.0);
    approx(&log["fat_g"], 7.5);
    assert_eq!(log["food_name"], "Chicken bowl");

    let (s, _) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({ "food_id": food_id, "servings": 0.5 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    let uri = format!("/food-logs?date={}", today());
    let (s, logs) = call(&app, Method::GET, &uri, None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(logs.as_array().unwrap().len(), 2);

    let (s, sum) = call(
        &app,
        Method::GET,
        &format!("/food-logs/summary?date={}", today()),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    approx(&sum["kcal"], 500.0);
    approx(&sum["protein_g"], 20.0);
    approx(&sum["carbs_g"], 40.0);
    approx(&sum["fat_g"], 10.0);
    assert_eq!(sum["entries"], 2);
}

#[sqlx::test]
async fn update_recomputes_totals_and_delete_is_soft(pool: PgPool) {
    let app = router(pool.clone());
    let t = token(&app, "ann@example.com").await;
    let food = custom_food(&app, &t, "Rice", 200.0).await;
    let (_, log) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({ "food_id": food["id"], "servings": 1.0 })),
        Some(&t),
    )
    .await;
    let id = log["id"].as_str().unwrap();

    let (s, upd) = call(
        &app,
        Method::PUT,
        &format!("/food-logs/{id}"),
        Some(json!({ "servings": 2.0, "meal": "dinner" })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{upd}");
    approx(&upd["kcal"], 400.0);
    assert_eq!(upd["meal"], "dinner");

    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/food-logs/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/food-logs/{id}"),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, sum) = call(
        &app,
        Method::GET,
        &format!("/food-logs/summary?date={}", today()),
        None,
        Some(&t),
    )
    .await;
    approx(&sum["kcal"], 0.0);

    // soft delete: the row is still there
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM food_logs WHERE deleted_at IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn logs_only_appear_on_their_own_day(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    let food = custom_food(&app, &t, "Soup", 100.0).await;
    let (s, _) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(
            json!({ "food_id": food["id"], "servings": 1.0, "logged_at": "2024-03-01T23:30:00Z" }),
        ),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, d1) = call(
        &app,
        Method::GET,
        "/food-logs?date=2024-03-01",
        None,
        Some(&t),
    )
    .await;
    let (_, d2) = call(
        &app,
        Method::GET,
        "/food-logs?date=2024-03-02",
        None,
        Some(&t),
    )
    .await;
    assert_eq!(d1.as_array().unwrap().len(), 1);
    assert_eq!(d2.as_array().unwrap().len(), 0);
    let (s, body) = call(&app, Method::GET, "/food-logs?date=nope", None, Some(&t)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[sqlx::test]
async fn search_merges_provider_results_and_caches_them(pool: PgPool) {
    let provider = fake(vec![oat(200.0)]);
    let app = router_with(
        pool.clone(),
        vec![Arc::new(FailingProvider), provider.clone()],
    );
    let t = token(&app, "ann@example.com").await;
    let mine = custom_food(&app, &t, "Oat bar", 150.0).await;

    // The failing provider degrades gracefully; the fake still contributes.
    let (s, res) = call(&app, Method::GET, "/foods/search?q=oat", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let res = res.as_array().unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(res[0]["id"], mine["id"], "own custom foods come first");
    assert_eq!(res[1]["name"], "Oat Granola");
    assert_eq!(res[1]["source"], "off");
    let cached_id = res[1]["id"].as_str().unwrap().to_string();

    // Repeat search: upsert keeps one row and the same id; no duplicates.
    let (_, res2) = call(&app, Method::GET, "/foods/search?q=oat", None, Some(&t)).await;
    assert_eq!(res2.as_array().unwrap().len(), 2);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM foods WHERE source = 'off'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);

    // Logs reference the cached local row.
    let (s, log) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({ "food_id": cached_id, "servings": 2.0 })),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{log}");
    approx(&log["kcal"], 400.0);

    // Updated provider data refreshes the cache but not the logged snapshot.
    let app2 = router_with(pool.clone(), vec![fake(vec![oat(300.0)])]);
    let (_, res3) = call(
        &app2,
        Method::GET,
        "/foods/search?q=granola",
        None,
        Some(&t),
    )
    .await;
    approx(&res3[0]["kcal"], 300.0);
    assert_eq!(res3[0]["id"], cached_id.as_str());
    let (_, logs) = call(
        &app2,
        Method::GET,
        &format!("/food-logs?date={}", today()),
        None,
        Some(&t),
    )
    .await;
    approx(&logs[0]["kcal"], 400.0);

    // Another user finds the shared cache but never your custom food.
    let t2 = token(&app, "bob@example.com").await;
    let (_, res4) = call(&app, Method::GET, "/foods/search?q=oat", None, Some(&t2)).await;
    let names: Vec<&str> = res4
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Oat Granola"]);
}

#[sqlx::test]
async fn search_rejects_short_queries_and_needs_auth(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    for uri in [
        "/foods/search?q=a",
        "/foods/search?q=%20a%20",
        "/foods/search",
    ] {
        let (s, body) = call(&app, Method::GET, uri, None, Some(&t)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {body}");
        assert!(body["error"]["fields"]["q"].is_array());
    }
    let (s, _) = call(&app, Method::GET, "/foods/search?q=oats", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn search_treats_wildcards_literally(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    custom_food(&app, &t, "Plain rice", 100.0).await;
    let (_, res) = call(&app, Method::GET, "/foods/search?q=%25%25", None, Some(&t)).await;
    assert_eq!(res.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn favorites_add_list_remove(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    let f1 = custom_food(&app, &t, "Apple", 52.0).await;
    let f2 = custom_food(&app, &t, "Banana", 89.0).await;
    for f in [&f1, &f2] {
        let uri = format!("/favorites/{}", f["id"].as_str().unwrap());
        let (s, _) = call(&app, Method::PUT, &uri, None, Some(&t)).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
    }
    // idempotent
    let uri1 = format!("/favorites/{}", f1["id"].as_str().unwrap());
    let (s, _) = call(&app, Method::PUT, &uri1, None, Some(&t)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let (s, favs) = call(&app, Method::GET, "/favorites", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(favs.as_array().unwrap().len(), 2);

    let (s, _) = call(&app, Method::DELETE, &uri1, None, Some(&t)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, favs) = call(&app, Method::GET, "/favorites", None, Some(&t)).await;
    assert_eq!(favs.as_array().unwrap().len(), 1);
    assert_eq!(favs[0]["name"], "Banana");

    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/favorites/{}", Uuid::new_v4()),
        None,
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn recent_foods_are_distinct_and_latest_first(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    let a = custom_food(&app, &t, "Apple", 52.0).await;
    let b = custom_food(&app, &t, "Banana", 89.0).await;
    for (f, at) in [
        (&a, "2024-01-01T08:00:00Z"),
        (&b, "2024-01-02T08:00:00Z"),
        (&a, "2024-01-03T08:00:00Z"),
    ] {
        let (s, _) = call(
            &app,
            Method::POST,
            "/food-logs",
            Some(json!({ "food_id": f["id"], "servings": 1.0, "logged_at": at })),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (s, recent) = call(&app, Method::GET, "/foods/recent", None, Some(&t)).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = recent
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Apple", "Banana"]);
}

#[sqlx::test]
async fn validation_errors(pool: PgPool) {
    let app = router(pool);
    let t = token(&app, "ann@example.com").await;
    let good = json!({"name": "X", "kcal": 10.0, "protein_g": 1.0, "carbs_g": 1.0, "fat_g": 1.0, "serving_g": 100.0});

    let cases = [
        ("kcal", json!({"kcal": 5001.0})),
        ("kcal", json!({"kcal": -1.0})),
        ("protein_g", json!({"protein_g": 1000.5})),
        ("carbs_g", json!({"carbs_g": -0.1})),
        ("fat_g", json!({"fat_g": 2000.0})),
        ("serving_g", json!({"serving_g": 0.0})),
        ("serving_g", json!({"serving_g": -5.0})),
        ("name", json!({"name": ""})),
    ];
    for (field, patch) in cases {
        let mut body = good.clone();
        for (k, v) in patch.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (s, resp) = call(&app, Method::POST, "/foods", Some(body), Some(&t)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {resp}");
        assert!(resp["error"]["fields"][field].is_array(), "{field}: {resp}");
    }
    let (s, _) = call(
        &app,
        Method::POST,
        "/foods",
        Some(json!({"name": "x"})),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let food = custom_food(&app, &t, "Ok", 10.0).await;
    for body in [
        json!({"food_id": food["id"], "servings": 0.0}),
        json!({"food_id": food["id"], "servings": -1.0}),
        json!({"food_id": food["id"], "servings": 1.0, "meal": "brunch"}),
    ] {
        let (s, resp) = call(&app, Method::POST, "/food-logs", Some(body), Some(&t)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{resp}");
    }
    // unknown food
    let (s, _) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({"food_id": Uuid::new_v4(), "servings": 1.0})),
        Some(&t),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn cross_user_access_is_404(pool: PgPool) {
    let app = router(pool);
    let ann = token(&app, "ann@example.com").await;
    let bob = token(&app, "bob@example.com").await;
    let food = custom_food(&app, &ann, "Secret stew", 300.0).await;
    let (_, log) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({"food_id": food["id"], "servings": 1.0})),
        Some(&ann),
    )
    .await;
    let id = log["id"].as_str().unwrap();

    // Bob cannot log, favorite, edit or delete Ann's things.
    let (s, _) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({"food_id": food["id"], "servings": 1.0})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/favorites/{}", food["id"].as_str().unwrap()),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/food-logs/{id}"),
        Some(json!({"servings": 9.0})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        Method::DELETE,
        &format!("/food-logs/{id}"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Bob sees nothing of Ann's.
    let (_, logs) = call(
        &app,
        Method::GET,
        &format!("/food-logs?date={}", today()),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(logs.as_array().unwrap().len(), 0);
    let (_, sum) = call(
        &app,
        Method::GET,
        &format!("/food-logs/summary?date={}", today()),
        None,
        Some(&bob),
    )
    .await;
    approx(&sum["kcal"], 0.0);
    let (_, res) = call(&app, Method::GET, "/foods/search?q=stew", None, Some(&bob)).await;
    assert_eq!(res.as_array().unwrap().len(), 0);

    // Ann's log is untouched.
    let (_, logs) = call(
        &app,
        Method::GET,
        &format!("/food-logs?date={}", today()),
        None,
        Some(&ann),
    )
    .await;
    approx(&logs[0]["servings"], 1.0);
}

#[sqlx::test]
async fn client_supplied_id_is_idempotent(pool: PgPool) {
    let app = router(pool);
    let ann = token(&app, "ann@example.com").await;
    let bob = token(&app, "bob@example.com").await;
    let food = custom_food(&app, &ann, "Pasta", 100.0).await;
    let id = Uuid::new_v4().to_string();
    let body = json!({"id": id, "food_id": food["id"], "servings": 1.0});

    let (s, first) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(body.clone()),
        Some(&ann),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(first["id"], id.as_str());
    let (s, again) = call(&app, Method::POST, "/food-logs", Some(body), Some(&ann)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again["logged_at"], first["logged_at"]);

    // Replay with changed servings updates the same row.
    let (s, changed) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({"id": id, "food_id": food["id"], "servings": 3.0})),
        Some(&ann),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    approx(&changed["kcal"], 300.0);

    let (_, logs) = call(
        &app,
        Method::GET,
        &format!("/food-logs?date={}", today()),
        None,
        Some(&ann),
    )
    .await;
    assert_eq!(logs.as_array().unwrap().len(), 1);

    // Someone else cannot take over the id.
    let bobs = custom_food(&app, &bob, "Bob food", 1.0).await;
    let (s, _) = call(
        &app,
        Method::POST,
        "/food-logs",
        Some(json!({"id": id, "food_id": bobs["id"], "servings": 1.0})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, logs) = call(
        &app,
        Method::GET,
        &format!("/food-logs?date={}", today()),
        None,
        Some(&ann),
    )
    .await;
    assert_eq!(logs[0]["food_name"], "Pasta");
}

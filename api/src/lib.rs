pub mod auth;
pub mod config;
pub mod error;
pub mod extract;

use axum::{routing::get, Router};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<config::Config>,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/auth", auth::routes())
        .route("/me", get(auth::me))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

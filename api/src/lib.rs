pub mod auth;
pub mod chat;
pub mod config;
pub mod dashboard;
pub mod error;
pub mod export;
pub mod extract;
pub mod food;
pub mod friends;
pub mod health;
pub mod leaderboard;
pub mod profile;
pub mod readiness;
pub mod sleep;
pub mod stats;
pub mod summary;
pub mod sync;
pub mod weight;
pub mod workouts;

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
        .merge(profile::routes())
        .merge(workouts::routes())
        .merge(food::routes())
        .merge(sleep::routes())
        .merge(weight::routes())
        .merge(sync::routes())
        .merge(summary::routes())
        .merge(stats::routes())
        .merge(chat::routes())
        .merge(leaderboard::routes())
        .merge(health::routes())
        .merge(friends::routes())
        .merge(export::routes())
        .merge(dashboard::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

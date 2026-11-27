//! sleep feature module. See docs/spec.md. Routes use full paths (e.g. "/sleep").
use crate::AppState;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new()
}

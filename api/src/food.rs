//! food feature module. See docs/spec.md. Routes use full paths (e.g. "/food").
use crate::AppState;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new()
}

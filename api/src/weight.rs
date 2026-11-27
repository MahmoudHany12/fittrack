//! weight feature module. See docs/spec.md. Routes use full paths (e.g. "/weight").
use crate::AppState;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new()
}

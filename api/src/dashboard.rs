//! dashboard feature module. See docs/spec.md. Routes use full paths.
use crate::AppState;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new()
}

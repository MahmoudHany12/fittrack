use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Validation(BTreeMap<String, Vec<String>>),
    Unauthorized,
    Conflict(&'static str),
    Internal(anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(e: validator::ValidationErrors) -> Self {
        let fields = e
            .field_errors()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|x| x.code.to_string()).collect()))
            .collect();
        AppError::Validation(fields)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, fields) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m, None),
            AppError::Validation(f) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
                "Some fields are invalid".to_string(),
                Some(f),
            ),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Invalid or missing credentials".to_string(),
                None,
            ),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.to_string(), None),
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Something went wrong".to_string(),
                    None,
                )
            }
        };
        let mut err = json!({ "code": code, "message": message });
        if let Some(f) = fields {
            err["fields"] = json!(f);
        }
        (status, Json(json!({ "error": err }))).into_response()
    }
}

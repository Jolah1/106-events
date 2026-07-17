use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("too many requests, try again shortly")]
    RateLimited,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(anyhow::Error::new(err).context("database error"))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Internal(err) => {
                tracing::error!("internal error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let message = match &self {
            // Never leak internal error details to clients.
            Self::Internal(_) => "something went wrong".to_string(),
            other => other.to_string(),
        };
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}

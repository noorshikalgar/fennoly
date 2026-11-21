//! API errors. Every handler returns `AppResult<T>`; errors become JSON
//! `{ "error": "<code>", "message": "<text>" }` with a matching status.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("sign in required")]
    Unauthorized,
    #[error("not allowed")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooLarge(String),
    #[error("too many attempts, try again in a minute")]
    TooManyRequests,
    #[error("{0}")]
    Unavailable(String),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::TooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if let AppError::Internal(err) = &self {
            tracing::error!(error = ?err, "request failed");
        }
        (status, Json(json!({ "error": code, "message": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
                AppError::Conflict("an item with this name already exists".into())
            }
            _ => AppError::Internal(err.into()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.into())
    }
}

impl From<fennoly_core::NameError> for AppError {
    fn from(err: fennoly_core::NameError) -> Self {
        AppError::BadRequest(err.to_string())
    }
}

impl From<fennoly_tools::ToolError> for AppError {
    fn from(err: fennoly_tools::ToolError) -> Self {
        use fennoly_tools::ToolError;
        match err {
            ToolError::NotAvailable => AppError::Unavailable(err.to_string()),
            ToolError::Unsupported | ToolError::Image(_) => AppError::BadRequest(err.to_string()),
            other => AppError::Internal(anyhow::anyhow!(other)),
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::Internal(err.into())
    }
}

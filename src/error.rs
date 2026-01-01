//! Error handling module
//!
//! Centralized error types and HTTP response conversion.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Application-wide Result type
pub type AppResult<T> = Result<T, AppError>;

/// Application error types
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // Client errors (4xx)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Account is frozen")]
    AccountFrozen,

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Unauthorized transfer: request user does not match sender")]
    UnauthorizedTransfer,

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Idempotency conflict: same key with different request")]
    IdempotencyConflict,

    #[error("Version conflict: concurrent modification detected")]
    VersionConflict,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Missing required header: {0}")]
    MissingHeader(String),

    // Domain errors
    #[error(transparent)]
    Domain(#[from] crate::domain::DomainError),

    // Server errors (5xx)
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),
}

/// Error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, details) = match &self {
            // 400 Bad Request
            AppError::InvalidRequest(msg) => {
                (StatusCode::BAD_REQUEST, "invalid_request", Some(msg.clone()))
            }
            AppError::InsufficientBalance => {
                (StatusCode::BAD_REQUEST, "insufficient_balance", None)
            }
            AppError::AccountFrozen => {
                (StatusCode::BAD_REQUEST, "account_frozen", None)
            }

            // 401 Unauthorized
            AppError::InvalidApiKey => {
                (StatusCode::UNAUTHORIZED, "invalid_api_key", None)
            }

            // 403 Forbidden
            AppError::PermissionDenied => {
                (StatusCode::FORBIDDEN, "permission_denied", None)
            }
            AppError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, "forbidden", Some(msg.clone()))
            }
            AppError::UnauthorizedTransfer => {
                (StatusCode::FORBIDDEN, "unauthorized_transfer", None)
            }

            // 404 Not Found
            AppError::UserNotFound(id) => {
                (StatusCode::NOT_FOUND, "user_not_found", Some(id.clone()))
            }
            AppError::AccountNotFound(id) => {
                (StatusCode::NOT_FOUND, "account_not_found", Some(id.clone()))
            }

            // 409 Conflict
            AppError::IdempotencyConflict => {
                (StatusCode::CONFLICT, "idempotency_conflict", None)
            }
            AppError::VersionConflict => {
                (StatusCode::CONFLICT, "version_conflict", None)
            }

            // 429 Too Many Requests
            AppError::RateLimitExceeded => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded", None)
            }

            // 400 Missing Header
            AppError::MissingHeader(header) => {
                (StatusCode::BAD_REQUEST, "missing_header", Some(header.clone()))
            }

            // Domain errors - map to appropriate HTTP status
            AppError::Domain(ref domain_err) => {
                use crate::domain::DomainError;
                match domain_err {
                    DomainError::InsufficientBalance { .. } => {
                        (StatusCode::BAD_REQUEST, "insufficient_balance", Some(domain_err.to_string()))
                    }
                    DomainError::AccountFrozen { .. } => {
                        (StatusCode::BAD_REQUEST, "account_frozen", Some(domain_err.to_string()))
                    }
                    DomainError::AccountNotActive => {
                        (StatusCode::BAD_REQUEST, "account_not_active", None)
                    }
                    DomainError::InvalidAmount(msg) => {
                        (StatusCode::BAD_REQUEST, "invalid_amount", Some(msg.clone()))
                    }
                    DomainError::UserNotFound(id) => {
                        (StatusCode::NOT_FOUND, "user_not_found", Some(id.clone()))
                    }
                    DomainError::AccountNotFound(id) => {
                        (StatusCode::NOT_FOUND, "account_not_found", Some(id.clone()))
                    }
                    DomainError::SameAccountTransfer => {
                        (StatusCode::BAD_REQUEST, "same_account_transfer", None)
                    }
                    DomainError::Unauthorized(msg) => {
                        (StatusCode::FORBIDDEN, "unauthorized", Some(msg.clone()))
                    }
                    DomainError::BusinessRuleViolation(msg) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "business_rule_violation", Some(msg.clone()))
                    }
                    DomainError::VersionConflict { expected, found } => {
                        (StatusCode::CONFLICT, "version_conflict", Some(format!("expected {}, found {}", expected, found)))
                    }
                    DomainError::DuplicateOperation { key } => {
                        (StatusCode::CONFLICT, "duplicate_operation", Some(key.clone()))
                    }
                }
            }

            // 500 Internal Server Error
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "database_error", None)
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", None)
            }
            AppError::Config(e) => {
                tracing::error!("Config error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "config_error", None)
            }
        };

        let body = ErrorResponse {
            error: self.to_string(),
            error_code: error_code.to_string(),
            details,
        };

        (status, Json(body)).into_response()
    }
}

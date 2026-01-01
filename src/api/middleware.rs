//! API Middleware
//!
//! Authentication and rate limiting middleware.

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::OperationContext;

/// API Key authentication result
#[derive(Debug, Clone)]
pub struct AuthenticatedApiKey {
    pub id: Uuid,
    pub name: String,
    pub permissions: Vec<String>,
}

impl AuthenticatedApiKey {
    /// Check if this API key has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission || p == "admin")
    }
}

/// Request user from X-Request-User-Id header
#[derive(Debug, Clone)]
pub struct RequestUser {
    pub user_id: Uuid,
}

// =========================================================================
// M114: API Key Authentication Middleware
// =========================================================================

/// Extract and validate API key from X-API-Key header
pub async fn auth_middleware(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // Extract API key
    let api_key = match headers.get("X-API-Key").and_then(|v| v.to_str().ok()) {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Missing X-API-Key header",
                    "error_code": "missing_api_key"
                })),
            )
                .into_response());
        }
    };

    // Validate API key
    let api_key_record: Option<(Uuid, String, Vec<String>, bool)> = match sqlx::query_as(
        r#"
        SELECT id, name, permissions, is_active
        FROM api_keys
        WHERE key_hash = encode(sha256($1::bytea), 'hex')
        "#,
    )
    .bind(api_key.as_bytes())
    .fetch_optional(&pool)
    .await
    {
        Ok(record) => record,
        Err(e) => {
            tracing::error!("Database error during API key validation: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "error_code": "database_error"
                })),
            )
                .into_response());
        }
    };

    let (api_key_id, name, permissions, is_active) = match api_key_record {
        Some(record) => record,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Invalid API key",
                    "error_code": "invalid_api_key"
                })),
            )
                .into_response());
        }
    };

    if !is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "API key is disabled",
                "error_code": "api_key_disabled"
            })),
        )
            .into_response());
    }

    // Store authenticated API key in request extensions
    request.extensions_mut().insert(AuthenticatedApiKey {
        id: api_key_id,
        name,
        permissions,
    });

    // Extract X-Request-User-Id if present
    // Note: Some endpoints require this header - they will check for RequestUser extension
    if let Some(user_id_str) = headers.get("X-Request-User-Id").and_then(|v| v.to_str().ok()) {
        match Uuid::parse_str(user_id_str) {
            Ok(user_id) => {
                request.extensions_mut().insert(RequestUser { user_id });
            }
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Invalid X-Request-User-Id header format",
                        "error_code": "invalid_user_id"
                    })),
                )
                    .into_response());
            }
        }
    }

    // Extract correlation ID or generate new one
    let correlation_id = headers
        .get("X-Correlation-Id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);

    // Build operation context
    let context = OperationContext::new()
        .with_api_key(api_key_id)
        .with_correlation_id(correlation_id);

    request.extensions_mut().insert(context);

    Ok(next.run(request).await)
}

// =========================================================================
// M115: Rate Limiting Middleware
// =========================================================================

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(pool): State<PgPool>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // Get API key from extensions
    let api_key = match request.extensions().get::<AuthenticatedApiKey>() {
        Some(key) => key.clone(),
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Auth middleware must run first",
                    "error_code": "internal_error"
                })),
            )
                .into_response());
        }
    };

    // Check rate limit
    let rate_limit = 100; // requests per minute
    let allowed: bool = match sqlx::query_scalar(
        r#"SELECT check_and_increment_rate_limit($1, $2)"#,
    )
    .bind(api_key.id)
    .bind(rate_limit)
    .fetch_one(&pool)
    .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Rate limit check error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Rate limit check failed",
                    "error_code": "database_error"
                })),
            )
                .into_response());
        }
    };

    if !allowed {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "Rate limit exceeded",
                "error_code": "rate_limit_exceeded"
            })),
        )
            .into_response());
    }

    Ok(next.run(request).await)
}

// =========================================================================
// M118: mask_headers_for_logging
// =========================================================================

/// Headers that should be masked in logs
const SENSITIVE_HEADERS: &[&str] = &[
    "x-api-key",
    "authorization",
    "cookie",
    "set-cookie",
];

/// Mask sensitive headers for logging
pub fn mask_headers_for_logging(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let name_lower = name.as_str().to_lowercase();
            let masked_value = if SENSITIVE_HEADERS.contains(&name_lower.as_str()) {
                "[REDACTED]".to_string()
            } else {
                value.to_str().unwrap_or("[invalid utf8]").to_string()
            };
            (name.to_string(), masked_value)
        })
        .collect()
}

// =========================================================================
// M119: Request Logging Middleware
// =========================================================================

/// Request logging middleware
pub async fn logging_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let version = request.version();
    
    // Mask sensitive headers
    let headers = mask_headers_for_logging(request.headers());
    
    // Extract correlation ID if available
    let correlation_id = request
        .extensions()
        .get::<crate::domain::OperationContext>()
        .map(|ctx| ctx.correlation_id)
        .flatten();
    
    let start = std::time::Instant::now();
    
    // Log request
    tracing::info!(
        method = %method,
        uri = %uri,
        version = ?version,
        correlation_id = ?correlation_id,
        headers = ?headers,
        "Incoming request"
    );
    
    // Process request
    let response = next.run(request).await;
    
    let duration = start.elapsed();
    let status = response.status();
    
    // Log response
    tracing::info!(
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = %duration.as_millis(),
        correlation_id = ?correlation_id,
        "Request completed"
    );
    
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::HeaderName;

    #[test]
    fn test_mask_headers_for_logging() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-api-key", "secret-key-12345".parse().unwrap());
        headers.insert("x-request-user-id", "user-123".parse().unwrap());
        
        let masked = mask_headers_for_logging(&headers);
        
        // Find each header in the result
        let api_key = masked.iter().find(|(k, _)| k == "x-api-key");
        let content_type = masked.iter().find(|(k, _)| k == "content-type");
        let user_id = masked.iter().find(|(k, _)| k == "x-request-user-id");
        
        assert_eq!(api_key.unwrap().1, "[REDACTED]");
        assert_eq!(content_type.unwrap().1, "application/json");
        assert_eq!(user_id.unwrap().1, "user-123");
    }

    #[test]
    fn test_sensitive_headers_list() {
        assert!(SENSITIVE_HEADERS.contains(&"x-api-key"));
        assert!(SENSITIVE_HEADERS.contains(&"authorization"));
        assert!(!SENSITIVE_HEADERS.contains(&"content-type"));
    }
}

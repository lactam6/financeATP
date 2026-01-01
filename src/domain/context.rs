//! Operation Context
//!
//! Contains metadata about the current operation for audit and tracing.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::net::IpAddr;

/// Context for an operation, used for auditing and tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationContext {
    /// API key ID used for this request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<Uuid>,

    /// User ID from X-Request-User-Id header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_user_id: Option<Uuid>,

    /// Correlation ID for request tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,

    /// Client IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<IpAddr>,
}

impl OperationContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self {
            api_key_id: None,
            request_user_id: None,
            correlation_id: None,
            client_ip: None,
        }
    }

    /// Create context with API key
    pub fn with_api_key(mut self, api_key_id: Uuid) -> Self {
        self.api_key_id = Some(api_key_id);
        self
    }

    /// Create context with request user ID
    pub fn with_request_user(mut self, user_id: Uuid) -> Self {
        self.request_user_id = Some(user_id);
        self
    }

    /// Create context with correlation ID
    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Create context with client IP
    pub fn with_client_ip(mut self, ip: IpAddr) -> Self {
        self.client_ip = Some(ip);
        self
    }

    /// Generate a new correlation ID if not present
    pub fn ensure_correlation_id(&mut self) -> Uuid {
        *self.correlation_id.get_or_insert_with(Uuid::new_v4)
    }
}

impl Default for OperationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder() {
        let api_key_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let context = OperationContext::new()
            .with_api_key(api_key_id)
            .with_request_user(user_id)
            .with_correlation_id(correlation_id);

        assert_eq!(context.api_key_id, Some(api_key_id));
        assert_eq!(context.request_user_id, Some(user_id));
        assert_eq!(context.correlation_id, Some(correlation_id));
    }

    #[test]
    fn test_ensure_correlation_id() {
        let mut context = OperationContext::new();
        assert!(context.correlation_id.is_none());

        let id = context.ensure_correlation_id();
        assert!(context.correlation_id.is_some());
        assert_eq!(context.correlation_id.unwrap(), id);

        // Calling again should return the same ID
        let id2 = context.ensure_correlation_id();
        assert_eq!(id, id2);
    }
}

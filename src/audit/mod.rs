//! Audit Log Service
//!
//! Provides tamper-evident audit logging with hash chain verification.
//! All operations are recorded for compliance and forensic analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::IpAddr;
use uuid::Uuid;

use crate::domain::OperationContext;

// =========================================================================
// M141: AuditLogService
// =========================================================================

/// Audit log entry for database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub sequence_number: i64,
    pub api_key_id: Option<Uuid>,
    pub request_user_id: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub changed_fields: Option<Vec<String>>,
    pub client_ip: Option<IpAddr>,
    pub previous_hash: String,
    pub current_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Audit action types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    UserCreated,
    UserUpdated,
    UserDeactivated,
    TransferExecuted,
    MintExecuted,
    BurnExecuted,
    ApiKeyCreated,
    ApiKeyRevoked,
    LoginAttempt,
    PermissionDenied,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::UserCreated => "user.created",
            AuditAction::UserUpdated => "user.updated",
            AuditAction::UserDeactivated => "user.deactivated",
            AuditAction::TransferExecuted => "transfer.executed",
            AuditAction::MintExecuted => "mint.executed",
            AuditAction::BurnExecuted => "burn.executed",
            AuditAction::ApiKeyCreated => "api_key.created",
            AuditAction::ApiKeyRevoked => "api_key.revoked",
            AuditAction::LoginAttempt => "auth.login_attempt",
            AuditAction::PermissionDenied => "auth.permission_denied",
        }
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Builder for creating audit log entries
#[derive(Debug, Clone)]
pub struct AuditLogBuilder {
    action: String,
    resource_type: Option<String>,
    resource_id: Option<Uuid>,
    before_state: Option<serde_json::Value>,
    after_state: Option<serde_json::Value>,
    changed_fields: Option<Vec<String>>,
}

impl AuditLogBuilder {
    /// Create a new audit log builder
    pub fn new(action: AuditAction) -> Self {
        Self {
            action: action.as_str().to_string(),
            resource_type: None,
            resource_id: None,
            before_state: None,
            after_state: None,
            changed_fields: None,
        }
    }

    /// Create with custom action string
    pub fn custom(action: &str) -> Self {
        Self {
            action: action.to_string(),
            resource_type: None,
            resource_id: None,
            before_state: None,
            after_state: None,
            changed_fields: None,
        }
    }

    /// Set the resource type
    pub fn resource_type(mut self, resource_type: &str) -> Self {
        self.resource_type = Some(resource_type.to_string());
        self
    }

    /// Set the resource ID
    pub fn resource_id(mut self, resource_id: Uuid) -> Self {
        self.resource_id = Some(resource_id);
        self
    }

    /// Set the before state
    pub fn before_state<T: Serialize>(mut self, state: &T) -> Self {
        self.before_state = serde_json::to_value(state).ok();
        self
    }

    /// Set the after state
    pub fn after_state<T: Serialize>(mut self, state: &T) -> Self {
        self.after_state = serde_json::to_value(state).ok();
        self
    }

    /// Set the changed fields
    pub fn changed_fields(mut self, fields: Vec<String>) -> Self {
        self.changed_fields = Some(fields);
        self
    }
}

/// Audit Log Service
#[derive(Debug, Clone)]
pub struct AuditLogService {
    pool: PgPool,
}

impl AuditLogService {
    /// Create a new AuditLogService
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // M142: Audit log write logic
    // =========================================================================

    /// Write an audit log entry
    /// The hash chain is calculated by the database trigger
    pub async fn log(
        &self,
        builder: AuditLogBuilder,
        context: &OperationContext,
    ) -> Result<Uuid, AuditLogError> {
        let id = Uuid::new_v4();

        // Convert changed_fields to PostgreSQL array format
        let changed_fields_array: Option<Vec<String>> = builder.changed_fields;

        // Note: sequence_number, previous_hash, and current_hash are set by the DB trigger
        let result: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO audit_logs (
                id, api_key_id, request_user_id, correlation_id,
                action, resource_type, resource_id,
                before_state, after_state, changed_fields, client_ip
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(context.api_key_id)
        .bind(context.request_user_id)
        .bind(context.correlation_id)
        .bind(&builder.action)
        .bind(&builder.resource_type)
        .bind(builder.resource_id)
        .bind(&builder.before_state)
        .bind(&builder.after_state)
        .bind(&changed_fields_array)
        .bind(context.client_ip.map(|ip| ip.to_string()))
        .fetch_one(&self.pool)
        .await?;

        tracing::debug!(
            audit_id = %result.0,
            action = %builder.action,
            "Audit log entry created"
        );

        Ok(result.0)
    }

    // =========================================================================
    // M143: Audit log verification (hash chain)
    // =========================================================================

    /// Verify the integrity of the audit log hash chain
    /// Returns Ok(true) if chain is valid, Ok(false) if tampered, Err on DB error
    pub async fn verify_hash_chain(&self, limit: Option<i64>) -> Result<ChainVerificationResult, AuditLogError> {
        let limit = limit.unwrap_or(1000);

        let entries: Vec<(Uuid, i64, String, String, String, Option<Uuid>, Option<serde_json::Value>, Option<serde_json::Value>)> = sqlx::query_as(
            r#"
            SELECT id, sequence_number, action, previous_hash, current_hash, 
                   request_user_id, before_state, after_state
            FROM audit_logs
            ORDER BY sequence_number ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        if entries.is_empty() {
            return Ok(ChainVerificationResult {
                is_valid: true,
                entries_checked: 0,
                first_invalid_entry: None,
                expected_hash: None,
                actual_hash: None,
            });
        }

        let mut previous_hash = "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        for (id, seq, action, prev_hash, current_hash, req_user_id, before_state, after_state) in &entries {
            // Verify chain linkage
            if prev_hash != &previous_hash {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    entries_checked: *seq as u64,
                    first_invalid_entry: Some(*id),
                    expected_hash: Some(previous_hash),
                    actual_hash: Some(prev_hash.clone()),
                });
            }

            // Recalculate hash
            let hash_input = format!(
                "{}{}{}{}{}{}{}",
                id,
                seq,
                action,
                req_user_id.map(|u| u.to_string()).unwrap_or_default(),
                before_state.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                after_state.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                prev_hash
            );

            let calculated_hash = sha256_hex(&hash_input);

            if &calculated_hash != current_hash {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    entries_checked: *seq as u64,
                    first_invalid_entry: Some(*id),
                    expected_hash: Some(calculated_hash),
                    actual_hash: Some(current_hash.clone()),
                });
            }

            previous_hash = current_hash.clone();
        }

        Ok(ChainVerificationResult {
            is_valid: true,
            entries_checked: entries.len() as u64,
            first_invalid_entry: None,
            expected_hash: None,
            actual_hash: None,
        })
    }

    /// Get recent audit logs
    pub async fn get_recent(&self, limit: i64) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        let entries: Vec<(
            Uuid, i64, Option<Uuid>, Option<Uuid>, Option<Uuid>,
            String, Option<String>, Option<Uuid>,
            Option<serde_json::Value>, Option<serde_json::Value>, Option<Vec<String>>,
            Option<String>, String, String, DateTime<Utc>
        )> = sqlx::query_as(
            r#"
            SELECT id, sequence_number, api_key_id, request_user_id, correlation_id,
                   action, resource_type, resource_id,
                   before_state, after_state, changed_fields,
                   client_ip::text, previous_hash, current_hash, created_at
            FROM audit_logs
            ORDER BY sequence_number DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries.into_iter().map(|(
            id, sequence_number, api_key_id, request_user_id, correlation_id,
            action, resource_type, resource_id,
            before_state, after_state, changed_fields,
            client_ip, previous_hash, current_hash, created_at
        )| {
            AuditLogEntry {
                id,
                sequence_number,
                api_key_id,
                request_user_id,
                correlation_id,
                action,
                resource_type,
                resource_id,
                before_state,
                after_state,
                changed_fields,
                client_ip: client_ip.and_then(|s| s.parse().ok()),
                previous_hash,
                current_hash,
                created_at,
            }
        }).collect())
    }

    /// Get audit logs for a specific user
    pub async fn get_by_user(&self, user_id: Uuid, limit: i64) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        let entries: Vec<(
            Uuid, i64, Option<Uuid>, Option<Uuid>, Option<Uuid>,
            String, Option<String>, Option<Uuid>,
            Option<serde_json::Value>, Option<serde_json::Value>, Option<Vec<String>>,
            Option<String>, String, String, DateTime<Utc>
        )> = sqlx::query_as(
            r#"
            SELECT id, sequence_number, api_key_id, request_user_id, correlation_id,
                   action, resource_type, resource_id,
                   before_state, after_state, changed_fields,
                   client_ip::text, previous_hash, current_hash, created_at
            FROM audit_logs
            WHERE request_user_id = $1
            ORDER BY sequence_number DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries.into_iter().map(|(
            id, sequence_number, api_key_id, request_user_id, correlation_id,
            action, resource_type, resource_id,
            before_state, after_state, changed_fields,
            client_ip, previous_hash, current_hash, created_at
        )| {
            AuditLogEntry {
                id,
                sequence_number,
                api_key_id,
                request_user_id,
                correlation_id,
                action,
                resource_type,
                resource_id,
                before_state,
                after_state,
                changed_fields,
                client_ip: client_ip.and_then(|s| s.parse().ok()),
                previous_hash,
                current_hash,
                created_at,
            }
        }).collect())
    }
}

/// Result of hash chain verification
#[derive(Debug, Clone)]
pub struct ChainVerificationResult {
    pub is_valid: bool,
    pub entries_checked: u64,
    pub first_invalid_entry: Option<Uuid>,
    pub expected_hash: Option<String>,
    pub actual_hash: Option<String>,
}

/// Calculate SHA-256 hash and return as hex string
fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Audit log errors
#[derive(Debug, thiserror::Error)]
pub enum AuditLogError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_action_as_str() {
        assert_eq!(AuditAction::UserCreated.as_str(), "user.created");
        assert_eq!(AuditAction::TransferExecuted.as_str(), "transfer.executed");
        assert_eq!(AuditAction::PermissionDenied.as_str(), "auth.permission_denied");
    }

    #[test]
    fn test_audit_log_builder() {
        let builder = AuditLogBuilder::new(AuditAction::UserCreated)
            .resource_type("User")
            .resource_id(Uuid::new_v4())
            .changed_fields(vec!["username".to_string(), "email".to_string()]);

        assert_eq!(builder.action, "user.created");
        assert_eq!(builder.resource_type, Some("User".to_string()));
        assert!(builder.changed_fields.is_some());
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex("test input");
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_chain_verification_result() {
        let result = ChainVerificationResult {
            is_valid: true,
            entries_checked: 100,
            first_invalid_entry: None,
            expected_hash: None,
            actual_hash: None,
        };

        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 100);
    }
}

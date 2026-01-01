//! Idempotency Repository
//!
//! Manages idempotency keys for preventing duplicate request processing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Idempotency key status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl From<String> for IdempotencyStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "pending" => IdempotencyStatus::Pending,
            "processing" => IdempotencyStatus::Processing,
            "completed" => IdempotencyStatus::Completed,
            "failed" => IdempotencyStatus::Failed,
            _ => IdempotencyStatus::Pending,
        }
    }
}

impl std::fmt::Display for IdempotencyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdempotencyStatus::Pending => write!(f, "pending"),
            IdempotencyStatus::Processing => write!(f, "processing"),
            IdempotencyStatus::Completed => write!(f, "completed"),
            IdempotencyStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Stored idempotency key information
#[derive(Debug, Clone)]
pub struct IdempotencyKey {
    pub key: Uuid,
    pub request_hash: String,
    pub event_id: Option<Uuid>,
    pub response_status: Option<i32>,
    pub response_body: Option<serde_json::Value>,
    pub status: IdempotencyStatus,
    pub processing_started_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Idempotency Repository Error
#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Key already exists and is being processed")]
    KeyInProgress,

    #[error("Request hash mismatch for key {0}")]
    HashMismatch(Uuid),

    #[error("Key not found: {0}")]
    NotFound(Uuid),
}

/// Repository for managing idempotency keys
#[derive(Debug, Clone)]
pub struct IdempotencyRepository {
    pool: PgPool,
}

impl IdempotencyRepository {
    /// Create a new IdempotencyRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // M092: get
    // =========================================================================

    /// Get an existing idempotency key
    pub async fn get(&self, key: Uuid) -> Result<Option<IdempotencyKey>, IdempotencyError> {
        let result: Option<(
            Uuid,
            String,
            Option<Uuid>,
            Option<i32>,
            Option<serde_json::Value>,
            String,
            Option<DateTime<Utc>>,
            DateTime<Utc>,
            DateTime<Utc>,
        )> = sqlx::query_as(
            r#"
            SELECT 
                key, request_hash, event_id, response_status, response_body,
                processing_status, processing_started_at, created_at, expires_at
            FROM idempotency_keys
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(
            |(key, request_hash, event_id, response_status, response_body, status, processing_started_at, created_at, expires_at)| {
                IdempotencyKey {
                    key,
                    request_hash,
                    event_id,
                    response_status,
                    response_body,
                    status: IdempotencyStatus::from(status),
                    processing_started_at,
                    created_at,
                    expires_at,
                }
            },
        ))
    }

    // =========================================================================
    // M093: start_processing
    // =========================================================================

    /// Start processing a new idempotency key
    /// Returns Ok(None) if key was successfully created
    /// Returns Ok(Some(key)) if key already exists (caller should check status)
    /// Returns Err if database error
    pub async fn start_processing(
        &self,
        key: Uuid,
        request_hash: &str,
    ) -> Result<Option<IdempotencyKey>, IdempotencyError> {
        // First, check if key exists
        if let Some(existing) = self.get(key).await? {
            // Verify request hash matches
            if existing.request_hash != request_hash {
                return Err(IdempotencyError::HashMismatch(key));
            }

            // If completed, caller can return cached response
            // If processing, caller should wait or retry
            // If failed, allow retry
            if existing.status == IdempotencyStatus::Processing {
                // Check if stuck (processing for more than 5 minutes)
                if let Some(started) = existing.processing_started_at {
                    let duration = Utc::now() - started;
                    if duration.num_minutes() < 5 {
                        return Err(IdempotencyError::KeyInProgress);
                    }
                    // If stuck, reset and allow retry below
                }
            }

            if existing.status == IdempotencyStatus::Completed {
                return Ok(Some(existing));
            }

            // Failed or stuck processing - update to processing
            sqlx::query(
                r#"
                UPDATE idempotency_keys
                SET processing_status = 'processing', processing_started_at = NOW()
                WHERE key = $1
                "#,
            )
            .bind(key)
            .execute(&self.pool)
            .await?;

            return Ok(None);
        }

        // Key doesn't exist - create new
        sqlx::query(
            r#"
            INSERT INTO idempotency_keys (key, request_hash, processing_status, processing_started_at)
            VALUES ($1, $2, 'processing', NOW())
            "#,
        )
        .bind(key)
        .bind(request_hash)
        .execute(&self.pool)
        .await?;

        Ok(None)
    }

    // =========================================================================
    // M094: mark_completed
    // =========================================================================

    /// Mark an idempotency key as completed with response
    pub async fn mark_completed(
        &self,
        key: Uuid,
        event_id: Uuid,
        response_status: i32,
        response_body: serde_json::Value,
    ) -> Result<(), IdempotencyError> {
        let rows = sqlx::query(
            r#"
            UPDATE idempotency_keys
            SET 
                processing_status = 'completed',
                event_id = $2,
                response_status = $3,
                response_body = $4
            WHERE key = $1
            "#,
        )
        .bind(key)
        .bind(event_id)
        .bind(response_status)
        .bind(response_body)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows == 0 {
            return Err(IdempotencyError::NotFound(key));
        }

        Ok(())
    }

    // =========================================================================
    // M095: mark_failed
    // =========================================================================

    /// Mark an idempotency key as failed
    pub async fn mark_failed(
        &self,
        key: Uuid,
        response_status: Option<i32>,
        response_body: Option<serde_json::Value>,
    ) -> Result<(), IdempotencyError> {
        let rows = sqlx::query(
            r#"
            UPDATE idempotency_keys
            SET 
                processing_status = 'failed',
                response_status = $2,
                response_body = $3
            WHERE key = $1
            "#,
        )
        .bind(key)
        .bind(response_status)
        .bind(response_body)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows == 0 {
            return Err(IdempotencyError::NotFound(key));
        }

        Ok(())
    }

    /// Check if a key exists and is completed
    pub async fn is_completed(&self, key: Uuid) -> Result<bool, IdempotencyError> {
        let status: Option<String> = sqlx::query_scalar(
            r#"
            SELECT processing_status FROM idempotency_keys WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(status.map(|s| s == "completed").unwrap_or(false))
    }

    /// Delete expired idempotency keys
    pub async fn cleanup_expired(&self) -> Result<u64, IdempotencyError> {
        let rows = sqlx::query(
            r#"
            DELETE FROM idempotency_keys
            WHERE expires_at < NOW()
            "#,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(rows)
    }

    /// Compute SHA-256 hash of request body for conflict detection
    pub fn compute_request_hash(body: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(body);
        hex::encode(hasher.finalize())
    }
}

// =========================================================================
// M096: Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotency_status_from_string() {
        assert_eq!(
            IdempotencyStatus::from("pending".to_string()),
            IdempotencyStatus::Pending
        );
        assert_eq!(
            IdempotencyStatus::from("processing".to_string()),
            IdempotencyStatus::Processing
        );
        assert_eq!(
            IdempotencyStatus::from("completed".to_string()),
            IdempotencyStatus::Completed
        );
        assert_eq!(
            IdempotencyStatus::from("failed".to_string()),
            IdempotencyStatus::Failed
        );
        assert_eq!(
            IdempotencyStatus::from("unknown".to_string()),
            IdempotencyStatus::Pending
        );
    }

    #[test]
    fn test_idempotency_status_display() {
        assert_eq!(IdempotencyStatus::Pending.to_string(), "pending");
        assert_eq!(IdempotencyStatus::Processing.to_string(), "processing");
        assert_eq!(IdempotencyStatus::Completed.to_string(), "completed");
        assert_eq!(IdempotencyStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_compute_request_hash() {
        let body = b"{\"amount\": \"100.00\"}";
        let hash = IdempotencyRepository::compute_request_hash(body);
        
        // Hash should be 64 hex characters (SHA-256)
        assert_eq!(hash.len(), 64);
        
        // Same input should produce same hash
        let hash2 = IdempotencyRepository::compute_request_hash(body);
        assert_eq!(hash, hash2);
        
        // Different input should produce different hash
        let hash3 = IdempotencyRepository::compute_request_hash(b"{\"amount\": \"200.00\"}");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_idempotency_error_display() {
        let err = IdempotencyError::KeyInProgress;
        assert!(err.to_string().contains("being processed"));

        let err = IdempotencyError::HashMismatch(Uuid::nil());
        assert!(err.to_string().contains("hash mismatch"));

        let err = IdempotencyError::NotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
    }
}

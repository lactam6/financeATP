//! Event Store Errors
//!
//! Error types for event store operations.

use uuid::Uuid;

/// Errors that can occur in the event store
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    /// Optimistic concurrency conflict
    #[error("Concurrency conflict for aggregate {aggregate_id}: expected version {expected}, found {actual}")]
    ConcurrencyConflict {
        aggregate_id: Uuid,
        expected: i64,
        actual: i64,
    },

    /// Aggregate not found
    #[error("Aggregate not found: {0}")]
    AggregateNotFound(Uuid),

    /// Idempotency key already exists
    #[error("Idempotency key already exists: {0}")]
    IdempotencyKeyExists(Uuid),

    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Maximum retries exceeded
    #[error("Maximum retries exceeded for atomic operation")]
    MaxRetriesExceeded,

    /// Invalid event data
    #[error("Invalid event data: {0}")]
    InvalidEventData(String),
}

impl EventStoreError {
    /// Check if this error is a concurrency conflict
    pub fn is_concurrency_conflict(&self) -> bool {
        matches!(self, EventStoreError::ConcurrencyConflict { .. })
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            EventStoreError::ConcurrencyConflict { .. } | EventStoreError::Database(_)
        )
    }
}

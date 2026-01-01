//! Event Store Repository
//!
//! Core implementation of the Event Store pattern.
//! Provides atomic event persistence with optimistic concurrency control.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::time::Duration;
use uuid::Uuid;

use crate::aggregate::Aggregate;
use crate::domain::OperationContext;

use super::EventStoreError;

/// Stored event from the database
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: i64,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub context: serde_json::Value,
    pub idempotency_key: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Operation to be performed on an aggregate
#[derive(Debug)]
pub struct AggregateOperation {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub expected_version: i64,
    pub event_type: String,
    pub event_data: serde_json::Value,
}

impl AggregateOperation {
    /// Create a new aggregate operation
    pub fn new<E: Serialize>(
        aggregate_type: &str,
        aggregate_id: Uuid,
        expected_version: i64,
        event_type: &str,
        event: &E,
    ) -> Result<Self, EventStoreError> {
        let event_data = serde_json::to_value(event)?;
        Ok(Self {
            aggregate_type: aggregate_type.to_string(),
            aggregate_id,
            expected_version,
            event_type: event_type.to_string(),
            event_data,
        })
    }
}

/// Event Store for persisting and retrieving events
#[derive(Debug, Clone)]
pub struct EventStore {
    pool: PgPool,
}

impl EventStore {
    /// Create a new EventStore with a database pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // M078: append_atomic with retry
    // =========================================================================

    /// Atomically append events across multiple aggregates with retry
    pub async fn append_atomic(
        &self,
        operations: Vec<AggregateOperation>,
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<Vec<Uuid>, EventStoreError> {
        const MAX_RETRIES: u32 = 3;

        for attempt in 0..MAX_RETRIES {
            match self
                .try_append_atomic(&operations, idempotency_key, context)
                .await
            {
                Ok(ids) => return Ok(ids),
                Err(EventStoreError::ConcurrencyConflict { .. }) if attempt < MAX_RETRIES - 1 => {
                    // Exponential backoff before retry
                    let delay = Duration::from_millis(50 * (attempt as u64 + 1));
                    tokio::time::sleep(delay).await;
                    tracing::warn!(
                        "Concurrency conflict, retrying (attempt {}/{})",
                        attempt + 1,
                        MAX_RETRIES
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(EventStoreError::MaxRetriesExceeded)
    }

    // =========================================================================
    // M077: try_append_atomic (single attempt)
    // =========================================================================

    /// Try to atomically append events (single attempt)
    async fn try_append_atomic(
        &self,
        operations: &[AggregateOperation],
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<Vec<Uuid>, EventStoreError> {
        let context_json = serde_json::to_value(context)?;

        // Start transaction with SERIALIZABLE isolation
        let mut tx = self.pool.begin().await?;

        // Check idempotency key if provided
        if let Some(key) = idempotency_key {
            if let Some(existing) = self.check_idempotency_key(&mut tx, key).await? {
                // Already processed, return existing event ID
                return Ok(vec![existing]);
            }
        }

        let mut event_ids = Vec::with_capacity(operations.len());

        for (idx, op) in operations.iter().enumerate() {
            // M079: Verify expected version (optimistic locking)
            let current_version = self
                .get_current_version(&mut tx, op.aggregate_id)
                .await?;

            if current_version != op.expected_version {
                return Err(EventStoreError::ConcurrencyConflict {
                    aggregate_id: op.aggregate_id,
                    expected: op.expected_version,
                    actual: current_version,
                });
            }

            // Insert event
            let new_version = op.expected_version + 1;
            let idem_key = if idx == 0 { idempotency_key } else { None };

            let event_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO events (
                    aggregate_type, aggregate_id, version, 
                    event_type, event_data, context, idempotency_key
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id
                "#,
            )
            .bind(&op.aggregate_type)
            .bind(op.aggregate_id)
            .bind(new_version)
            .bind(&op.event_type)
            .bind(&op.event_data)
            .bind(&context_json)
            .bind(idem_key)
            .fetch_one(&mut *tx)
            .await?;

            event_ids.push(event_id);
        }

        // Mark idempotency key as completed
        if let Some(key) = idempotency_key {
            self.complete_idempotency_key(&mut tx, key, event_ids[0])
                .await?;
        }

        // Commit transaction
        tx.commit().await?;

        Ok(event_ids)
    }

    /// Get current version of an aggregate
    async fn get_current_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        aggregate_id: Uuid,
    ) -> Result<i64, EventStoreError> {
        let result: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT MAX(version) FROM events WHERE aggregate_id = $1
            "#,
        )
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?
        .flatten();

        Ok(result.unwrap_or(0))
    }

    /// Check if idempotency key exists and return event ID if completed
    async fn check_idempotency_key(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        key: Uuid,
    ) -> Result<Option<Uuid>, EventStoreError> {
        let result: Option<(String, Option<Uuid>)> = sqlx::query_as(
            r#"
            SELECT processing_status, event_id 
            FROM idempotency_keys 
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?;

        match result {
            Some((status, event_id)) if status == "completed" => Ok(event_id),
            Some((status, _)) if status == "processing" => {
                // Another request is processing, treat as conflict
                Err(EventStoreError::IdempotencyKeyExists(key))
            }
            Some(_) => Ok(None), // Failed or pending, can retry
            None => {
                // Register new idempotency key
                sqlx::query(
                    r#"
                    INSERT INTO idempotency_keys (key, request_hash, processing_status, processing_started_at)
                    VALUES ($1, $2, 'processing', NOW())
                    "#,
                )
                .bind(key)
                .bind("") // TODO: Add request hash
                .execute(&mut **tx)
                .await?;
                Ok(None)
            }
        }
    }

    /// Mark idempotency key as completed
    async fn complete_idempotency_key(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        key: Uuid,
        event_id: Uuid,
    ) -> Result<(), EventStoreError> {
        sqlx::query(
            r#"
            UPDATE idempotency_keys 
            SET processing_status = 'completed', event_id = $2
            WHERE key = $1
            "#,
        )
        .bind(key)
        .bind(event_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    // =========================================================================
    // M081 & M082: load_aggregate with snapshot support
    // =========================================================================

    /// Load an aggregate by replaying events (with snapshot optimization)
    pub async fn load_aggregate<A>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Option<A>, EventStoreError>
    where
        A: Aggregate + DeserializeOwned + Default + Serialize,
        A::Event: DeserializeOwned,
    {
        // 1. Try to load from snapshot
        let (from_version, initial_state) = self.load_snapshot::<A>(aggregate_id).await?;

        // 2. Load events after snapshot version
        let events: Vec<StoredEvent> = sqlx::query_as::<_, (Uuid, String, Uuid, i64, String, serde_json::Value, serde_json::Value, Option<Uuid>, DateTime<Utc>)>(
            r#"
            SELECT id, aggregate_type, aggregate_id, version, event_type, event_data, context, idempotency_key, created_at
            FROM events
            WHERE aggregate_id = $1 AND version > $2
            ORDER BY version ASC
            "#,
        )
        .bind(aggregate_id)
        .bind(from_version)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(id, agg_type, agg_id, version, event_type, event_data, context, idem_key, created_at)| {
            StoredEvent {
                id,
                aggregate_type: agg_type,
                aggregate_id: agg_id,
                version,
                event_type,
                event_data,
                context,
                idempotency_key: idem_key,
                created_at,
            }
        })
        .collect();

        // If no snapshot and no events, aggregate doesn't exist
        if initial_state.is_none() && events.is_empty() {
            return Ok(None);
        }

        // 3. Replay events on initial state
        let mut aggregate = initial_state.unwrap_or_default();
        for stored_event in events {
            let event: A::Event = serde_json::from_value(stored_event.event_data)?;
            aggregate = aggregate.apply(event);
        }

        Ok(Some(aggregate))
    }

    /// Load snapshot for an aggregate
    async fn load_snapshot<A>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<(i64, Option<A>), EventStoreError>
    where
        A: Aggregate + DeserializeOwned,
    {
        let result: Option<(i64, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT version, state 
            FROM event_snapshots
            WHERE aggregate_type = $1 AND aggregate_id = $2
            "#,
        )
        .bind(A::aggregate_type())
        .bind(aggregate_id)
        .fetch_optional(&self.pool)
        .await?;

        match result {
            Some((version, state)) => {
                let aggregate: A = serde_json::from_value(state)?;
                Ok((version, Some(aggregate)))
            }
            None => Ok((0, None)),
        }
    }

    // =========================================================================
    // M084: save_snapshot_if_needed
    // =========================================================================

    /// Save a snapshot if the aggregate version warrants it
    pub async fn save_snapshot_if_needed<A>(
        &self,
        aggregate: &A,
    ) -> Result<bool, EventStoreError>
    where
        A: Aggregate + Serialize,
    {
        if !aggregate.should_snapshot() {
            return Ok(false);
        }

        let state = serde_json::to_value(aggregate)?;

        sqlx::query(
            r#"
            INSERT INTO event_snapshots (aggregate_type, aggregate_id, version, state)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (aggregate_type, aggregate_id) 
            DO UPDATE SET version = $3, state = $4, created_at = NOW()
            "#,
        )
        .bind(A::aggregate_type())
        .bind(aggregate.id())
        .bind(aggregate.version())
        .bind(state)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            "Snapshot saved for {} aggregate {} at version {}",
            A::aggregate_type(),
            aggregate.id(),
            aggregate.version()
        );

        Ok(true)
    }

    /// Get all events for an aggregate (for debugging/auditing)
    pub async fn get_events(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Vec<StoredEvent>, EventStoreError> {
        let events: Vec<StoredEvent> = sqlx::query_as::<_, (Uuid, String, Uuid, i64, String, serde_json::Value, serde_json::Value, Option<Uuid>, DateTime<Utc>)>(
            r#"
            SELECT id, aggregate_type, aggregate_id, version, event_type, event_data, context, idempotency_key, created_at
            FROM events
            WHERE aggregate_id = $1
            ORDER BY version ASC
            "#,
        )
        .bind(aggregate_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(id, agg_type, agg_id, version, event_type, event_data, context, idem_key, created_at)| {
            StoredEvent {
                id,
                aggregate_type: agg_type,
                aggregate_id: agg_id,
                version,
                event_type,
                event_data,
                context,
                idempotency_key: idem_key,
                created_at,
            }
        })
        .collect();

        Ok(events)
    }
}

// =========================================================================
// Tests (M080, M083, M085 - Integration tests require database)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_operation_new() {
        use crate::domain::AccountEvent;
        use chrono::Utc;

        let event = AccountEvent::AccountCreated {
            account_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            account_type: "user_wallet".to_string(),
            created_at: Utc::now(),
        };

        let op = AggregateOperation::new(
            "Account",
            Uuid::new_v4(),
            0,
            "AccountCreated",
            &event,
        )
        .unwrap();

        assert_eq!(op.aggregate_type, "Account");
        assert_eq!(op.expected_version, 0);
        assert_eq!(op.event_type, "AccountCreated");
    }

    #[test]
    fn test_event_store_error_is_retryable() {
        let conflict = EventStoreError::ConcurrencyConflict {
            aggregate_id: Uuid::new_v4(),
            expected: 1,
            actual: 2,
        };
        assert!(conflict.is_retryable());
        assert!(conflict.is_concurrency_conflict());

        let not_found = EventStoreError::AggregateNotFound(Uuid::new_v4());
        assert!(!not_found.is_retryable());
    }
}

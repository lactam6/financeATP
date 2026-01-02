//! Deactivate User Handler
//!
//! Handles user deactivation (soft delete) with event sourcing.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Aggregate, User};
use crate::domain::OperationContext;
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};

// =========================================================================
// DeactivateUserCommand
// =========================================================================

/// Command to deactivate a user
#[derive(Debug, Clone)]
pub struct DeactivateUserCommand {
    pub user_id: Uuid,
    pub reason: Option<String>,
}

impl DeactivateUserCommand {
    pub fn new(user_id: Uuid) -> Self {
        Self {
            user_id,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }
}

/// Result of a successful user deactivation
#[derive(Debug, Clone)]
pub struct DeactivateUserResult {
    pub user_id: Uuid,
    pub deactivated_at: DateTime<Utc>,
}

// =========================================================================
// DeactivateUserHandler
// =========================================================================

/// Handler for user deactivation
pub struct DeactivateUserHandler {
    event_store: EventStore,
    pool: PgPool,
}

impl DeactivateUserHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            pool,
        }
    }

    /// Execute the deactivate user command
    pub async fn execute(
        &self,
        command: DeactivateUserCommand,
        context: &OperationContext,
    ) -> Result<DeactivateUserResult, AppError> {
        // Load user aggregate from event store
        let user: User = self
            .event_store
            .load_aggregate(command.user_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .ok_or_else(|| AppError::UserNotFound(command.user_id.to_string()))?;

        // Check if user is system user
        let is_system: Option<bool> = sqlx::query_scalar("SELECT is_system FROM users WHERE id = $1")
            .bind(command.user_id)
            .fetch_optional(&self.pool)
            .await?;

        if is_system == Some(true) {
            return Err(AppError::Forbidden("Cannot deactivate system user".to_string()));
        }

        // Generate deactivate event
        let event = user.deactivate(command.reason)?;
        let deactivated_at = match &event {
            crate::domain::UserEvent::UserDeactivated { deactivated_at, .. } => *deactivated_at,
            _ => Utc::now(),
        };

        // Prepare operation
        let operation = AggregateOperation::new(
            "User",
            user.id(),
            user.version(),
            event.event_type(),
            &event,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

        // Persist event
        self.event_store
            .append_atomic(vec![operation], None, context)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Sync users table (projection)
        sqlx::query("UPDATE users SET is_active = false, updated_at = $2 WHERE id = $1")
            .bind(command.user_id)
            .bind(deactivated_at)
            .execute(&self.pool)
            .await?;

        Ok(DeactivateUserResult {
            user_id: command.user_id,
            deactivated_at,
        })
    }
}

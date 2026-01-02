//! Update User Handler
//!
//! Handles user profile updates with event sourcing.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Aggregate, User};
use crate::domain::{OperationContext, UserChanges};
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};

// =========================================================================
// UpdateUserCommand
// =========================================================================

/// Command to update a user's profile
#[derive(Debug, Clone)]
pub struct UpdateUserCommand {
    pub user_id: Uuid,
    pub changes: UserChanges,
}

impl UpdateUserCommand {
    pub fn new(user_id: Uuid, changes: UserChanges) -> Self {
        Self { user_id, changes }
    }
}

/// Result of a successful user update
#[derive(Debug, Clone)]
pub struct UpdateUserResult {
    pub user_id: Uuid,
    pub updated_at: DateTime<Utc>,
}

// =========================================================================
// UpdateUserHandler
// =========================================================================

/// Handler for user updates
pub struct UpdateUserHandler {
    event_store: EventStore,
    pool: PgPool,
}

impl UpdateUserHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            pool,
        }
    }

    /// Execute the update user command
    pub async fn execute(
        &self,
        command: UpdateUserCommand,
        context: &OperationContext,
    ) -> Result<UpdateUserResult, AppError> {
        // Load user aggregate from event store
        let user: User = self
            .event_store
            .load_aggregate(command.user_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .ok_or_else(|| AppError::UserNotFound(command.user_id.to_string()))?;

        // Generate update event
        let event = user.update(command.changes)?;
        let updated_at = match &event {
            crate::domain::UserEvent::UserUpdated { updated_at, .. } => *updated_at,
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
        let applied_user = user.apply(event);
        sqlx::query(
            r#"
            UPDATE users 
            SET display_name = $2, email = $3, updated_at = $4
            WHERE id = $1
            "#,
        )
        .bind(command.user_id)
        .bind(applied_user.display_name())
        .bind(applied_user.email())
        .bind(updated_at)
        .execute(&self.pool)
        .await?;

        Ok(UpdateUserResult {
            user_id: command.user_id,
            updated_at,
        })
    }
}

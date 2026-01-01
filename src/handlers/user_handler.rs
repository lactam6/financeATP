//! User Creation Handler
//!
//! Handles user creation with automatic wallet account creation.

use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Account, Aggregate, User};
use crate::domain::{AccountEvent, OperationContext, UserEvent};
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};
use crate::projection::ProjectionService;

use super::{CreateUserCommand, CreateUserResult};

// =========================================================================
// M098 & M099: CreateUserHandler
// =========================================================================

/// Handler for user creation
pub struct CreateUserHandler {
    event_store: EventStore,
    projection: ProjectionService,
    pool: PgPool,
}

impl CreateUserHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            projection: ProjectionService::new(pool.clone()),
            pool,
        }
    }

    /// Execute the create user command
    pub async fn execute(
        &self,
        command: CreateUserCommand,
        context: &OperationContext,
    ) -> Result<CreateUserResult, AppError> {
        // Check if user already exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM users WHERE id = $1 OR username = $2 OR email = $3"
        )
        .bind(command.user_id)
        .bind(&command.username)
        .bind(&command.email)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Err(AppError::InvalidRequest(
                "User with this ID, username, or email already exists".to_string(),
            ));
        }

        // Create user aggregate and event
        let (user, user_event) = User::create(
            command.user_id,
            command.username.clone(),
            command.email,
            command.display_name,
        );

        // M099: Create wallet account
        let account_id = Uuid::new_v4();
        let (account, account_event) = Account::create(
            account_id,
            command.user_id,
            "user_wallet".to_string(),
        );

        // Prepare atomic operations
        let operations = vec![
            AggregateOperation::new(
                "User",
                user.id(),
                0,
                user_event.event_type(),
                &user_event,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
            AggregateOperation::new(
                "Account",
                account.id(),
                0,
                account_event.event_type(),
                &account_event,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
        ];

        // Persist events atomically
        let event_ids = self
            .event_store
            .append_atomic(operations, None, context)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Insert user record (for queries)
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, display_name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, NOW(), NOW())
            "#,
        )
        .bind(command.user_id)
        .bind(&command.username)
        .bind(&user.email())
        .bind(user.display_name())
        .execute(&self.pool)
        .await?;

        // Insert account record
        sqlx::query(
            r#"
            INSERT INTO accounts (id, user_id, account_type)
            VALUES ($1, $2, 'user_wallet')
            "#,
        )
        .bind(account_id)
        .bind(command.user_id)
        .execute(&self.pool)
        .await?;

        // Create balance projection
        self.projection
            .create_account_balance(account_id, event_ids[1])
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Save snapshots if needed
        self.event_store
            .save_snapshot_if_needed(&user)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.event_store
            .save_snapshot_if_needed(&account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(CreateUserResult {
            user_id: command.user_id,
            account_id,
            username: command.username,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_user_command() {
        let cmd = CreateUserCommand::new(
            Uuid::new_v4(),
            "alice".to_string(),
            "alice@example.com".to_string(),
        )
        .with_display_name("Alice Smith".to_string());

        assert_eq!(cmd.username, "alice");
        assert_eq!(cmd.display_name, Some("Alice Smith".to_string()));
    }
}

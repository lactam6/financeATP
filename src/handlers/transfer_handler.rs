//! Transfer Handler
//!
//! Handles ATP transfers between users with full validation.

use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Account, Aggregate};
use crate::domain::{AccountEvent, Amount, OperationContext};
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};
use crate::idempotency::IdempotencyRepository;
use crate::projection::ProjectionService;

use super::{TransferCommand, TransferResult};

// =========================================================================
// M102: TransferHandler
// =========================================================================

/// Handler for ATP transfers
pub struct TransferHandler {
    event_store: EventStore,
    projection: ProjectionService,
    idempotency: IdempotencyRepository,
    pool: PgPool,
}

impl TransferHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            projection: ProjectionService::new(pool.clone()),
            idempotency: IdempotencyRepository::new(pool.clone()),
            pool,
        }
    }

    /// Execute the transfer command
    pub async fn execute(
        &self,
        command: TransferCommand,
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<TransferResult, AppError> {
        // M103: Authorization check
        if let Some(request_user_id) = context.request_user_id {
            if request_user_id != command.from_user_id {
                return Err(AppError::UnauthorizedTransfer);
            }
        } else {
            return Err(AppError::MissingHeader("X-Request-User-Id".to_string()));
        }

        // Validate same account transfer
        if command.from_user_id == command.to_user_id {
            return Err(AppError::InvalidRequest(
                "Cannot transfer to the same account".to_string(),
            ));
        }

        // Parse and validate amount
        let amount: Amount = command
            .amount
            .parse()
            .map_err(|e| AppError::InvalidRequest(format!("Invalid amount: {}", e)))?;

        // M104: Resolve user_id to account_id
        let from_account_id = self.get_wallet_account_id(command.from_user_id).await?;
        let to_account_id = self.get_wallet_account_id(command.to_user_id).await?;

        // Load sender's account
        let from_account: Account = self
            .event_store
            .load_aggregate(from_account_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .ok_or_else(|| AppError::AccountNotFound(from_account_id.to_string()))?;

        // Load recipient's account
        let to_account: Account = self
            .event_store
            .load_aggregate(to_account_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .ok_or_else(|| AppError::AccountNotFound(to_account_id.to_string()))?;

        // Generate transfer ID
        let transfer_id = Uuid::new_v4();

        // Generate debit event (from sender)
        let debit_event = from_account.debit(
            &amount,
            transfer_id,
            command.memo.clone().unwrap_or_else(|| "Transfer".to_string()),
        )?;

        // Generate credit event (to recipient)
        let credit_event = to_account.credit(
            &amount,
            transfer_id,
            command.memo.unwrap_or_else(|| "Transfer".to_string()),
        )?;

        // Prepare atomic operations
        let operations = vec![
            AggregateOperation::new(
                "Account",
                from_account_id,
                from_account.version(),
                debit_event.event_type(),
                &debit_event,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
            AggregateOperation::new(
                "Account",
                to_account_id,
                to_account.version(),
                credit_event.event_type(),
                &credit_event,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
        ];

        // Persist events atomically
        let event_ids = self
            .event_store
            .append_atomic(operations, idempotency_key, context)
            .await
            .map_err(|e| match e {
                crate::event_store::EventStoreError::ConcurrencyConflict { .. } => {
                    AppError::VersionConflict
                }
                crate::event_store::EventStoreError::IdempotencyKeyExists(_) => {
                    AppError::IdempotencyConflict
                }
                _ => AppError::Internal(e.to_string()),
            })?;

        // Update projections
        self.projection
            .apply_transfer(
                transfer_id,
                event_ids[0],
                from_account_id,
                to_account_id,
                &amount,
                from_account.version() + 1,
            )
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Apply events to get updated accounts
        let from_account = from_account.apply(debit_event);
        let to_account = to_account.apply(credit_event);

        // Save snapshots if needed
        self.event_store
            .save_snapshot_if_needed(&from_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.event_store
            .save_snapshot_if_needed(&to_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(TransferResult {
            transfer_id,
            from_user_id: command.from_user_id,
            to_user_id: command.to_user_id,
            amount: amount.value(),
            status: "completed".to_string(),
        })
    }

    // M104: user_id → account_id conversion
    async fn get_wallet_account_id(&self, user_id: Uuid) -> Result<Uuid, AppError> {
        let account_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM accounts 
            WHERE user_id = $1 AND account_type = 'user_wallet'
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        account_id.ok_or_else(|| AppError::UserNotFound(user_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_command() {
        let cmd = TransferCommand::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "100.00".to_string(),
        )
        .with_memo("Test payment".to_string());

        assert_eq!(cmd.amount, "100.00");
        assert_eq!(cmd.memo, Some("Test payment".to_string()));
    }
}

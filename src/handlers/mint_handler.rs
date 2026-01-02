//! Mint Handler
//!
//! Handles ATP minting (creation) from SYSTEM_MINT account.

use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Account, Aggregate};
use crate::domain::{Amount, OperationContext};
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};
use crate::projection::ProjectionService;

use super::{MintCommand, MintResult};

/// System user IDs (must match database seed)
const SYSTEM_MINT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

// =========================================================================
// M109: MintHandler
// =========================================================================

/// Handler for ATP minting
pub struct MintHandler {
    event_store: EventStore,
    projection: ProjectionService,
    pool: PgPool,
}

impl MintHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            projection: ProjectionService::new(pool.clone()),
            pool,
        }
    }

    /// Execute the mint command
    pub async fn execute(
        &self,
        command: MintCommand,
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<MintResult, AppError> {
        // Parse and validate amount
        let amount: Amount = command
            .amount
            .parse()
            .map_err(|e| AppError::InvalidRequest(format!("Invalid amount: {}", e)))?;

        // M110: Get SYSTEM_MINT account
        let system_mint_user_id: Uuid = SYSTEM_MINT_USER_ID
            .parse()
            .expect("Invalid SYSTEM_MINT_USER_ID");

        let mint_account_id = self.get_system_account_id(system_mint_user_id).await?;

        // Get recipient's wallet account
        let recipient_account_id = self.get_wallet_account_id(command.recipient_user_id).await?;

        // Load SYSTEM_MINT account from DB (bypasses event sourcing for system accounts)
        let mint_account = self.load_system_account(mint_account_id).await?;

        // Load recipient's account (use event sourcing if available, fallback to DB)
        let recipient_account = self.load_account_with_fallback(recipient_account_id).await?;

        // Generate mint ID
        let mint_id = Uuid::new_v4();

        // For minting, SYSTEM_MINT is debited (creates liability)
        // and recipient is credited
        let debit_description = format!("Mint: {}", command.reason);
        let credit_description = format!("Received from mint: {}", command.reason);

        // Note: SYSTEM_MINT can go negative (it's a liability account)
        // We bypass the normal debit check by directly creating the event
        let debit_event = crate::domain::AccountEvent::MoneyDebited {
            account_id: mint_account_id,
            amount: amount.value(),
            transfer_id: mint_id,
            description: debit_description,
            debited_at: chrono::Utc::now(),
        };

        let credit_event = recipient_account
            .credit(&amount, mint_id, credit_description)?;

        // Prepare atomic operations
        let operations = vec![
            AggregateOperation::new(
                "Account",
                mint_account_id,
                mint_account.version(),
                "MoneyDebited",
                &debit_event,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
            AggregateOperation::new(
                "Account",
                recipient_account_id,
                recipient_account.version(),
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
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Check for idempotency early return (only 1 event ID returned for 2 operations means cached)
        if event_ids.len() == 1 && idempotency_key.is_some() {
            // This was an idempotent request - return cached result (skip projection update)
            // Note: The original mint_id is not stored, so we generate a new one for the response
            // In production, you'd want to store and retrieve the original response
            return Ok(MintResult {
                mint_id: event_ids[0], // Use the cached event ID as mint_id
                recipient_user_id: command.recipient_user_id,
                amount: amount.value(),
            });
        }

        // Update projections (only for new requests)
        self.projection
            .apply_mint(
                mint_id,
                event_ids[0],
                mint_account_id,
                recipient_account_id,
                &amount,
                mint_account.version() + 1,
            )
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Apply events to get updated accounts
        let mint_account = mint_account.apply(debit_event);
        let recipient_account = recipient_account.apply(credit_event);

        // Save snapshots if needed
        self.event_store
            .save_snapshot_if_needed(&mint_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.event_store
            .save_snapshot_if_needed(&recipient_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(MintResult {
            mint_id,
            recipient_user_id: command.recipient_user_id,
            amount: amount.value(),
        })
    }

    async fn get_system_account_id(&self, user_id: Uuid) -> Result<Uuid, AppError> {
        let account_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM accounts 
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        account_id.ok_or_else(|| AppError::Internal("System account not found".to_string()))
    }

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

    /// Load system account directly from DB (bypasses event sourcing)
    async fn load_system_account(&self, account_id: Uuid) -> Result<Account, AppError> {
        // Get account info from DB
        let account_info: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as(
            r#"
            SELECT id, user_id, account_type, is_active
            FROM accounts
            WHERE id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        let (id, user_id, account_type, _is_active) = account_info
            .ok_or_else(|| AppError::Internal("SYSTEM_MINT account not found".to_string()))?;

        // Get current balance from projection
        let balance: Option<rust_decimal::Decimal> = sqlx::query_scalar(
            "SELECT balance FROM account_balances WHERE account_id = $1"
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        // Get current version from events
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM events WHERE aggregate_id = $1"
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;

        // Construct Account from DB state
        Ok(Account::from_db_state(id, user_id, account_type, balance.unwrap_or_default(), version))
    }

    /// Load account with event sourcing, fallback to DB if no events exist
    async fn load_account_with_fallback(&self, account_id: Uuid) -> Result<Account, AppError> {
        // Try event sourcing first
        match self.event_store.load_aggregate::<Account>(account_id).await {
            Ok(Some(account)) => Ok(account),
            Ok(None) => {
                // No events found, load from DB (for newly created accounts)
                self.load_system_account(account_id).await
                    .map_err(|_| AppError::AccountNotFound(account_id.to_string()))
            }
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mint_command() {
        let cmd = MintCommand::new(
            Uuid::new_v4(),
            "1000.00".to_string(),
            "Initial balance".to_string(),
        );

        assert_eq!(cmd.amount, "1000.00");
        assert_eq!(cmd.reason, "Initial balance");
    }

    #[test]
    fn test_system_mint_user_id() {
        let id: Uuid = SYSTEM_MINT_USER_ID.parse().unwrap();
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000001");
    }
}

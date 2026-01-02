//! Burn Handler
//!
//! Handles ATP burning (removal from circulation) to SYSTEM_BURN account.

use sqlx::PgPool;
use uuid::Uuid;

use crate::aggregate::{Account, Aggregate};
use crate::domain::{Amount, OperationContext};
use crate::error::AppError;
use crate::event_store::{AggregateOperation, EventStore};
use crate::projection::ProjectionService;

/// System burn user ID (must match database seed)
const SYSTEM_BURN_USER_ID: &str = "00000000-0000-0000-0000-000000000002";

/// Command to burn ATP
#[derive(Debug, Clone)]
pub struct BurnCommand {
    /// User ID to burn ATP from
    pub from_user_id: Uuid,
    /// Amount to burn
    pub amount: String,
    /// Reason for burning
    pub reason: String,
}

impl BurnCommand {
    pub fn new(from_user_id: Uuid, amount: String, reason: String) -> Self {
        Self {
            from_user_id,
            amount,
            reason,
        }
    }
}

/// Result of a successful burn
#[derive(Debug, Clone)]
pub struct BurnResult {
    pub burn_id: Uuid,
    pub from_user_id: Uuid,
    pub amount: rust_decimal::Decimal,
}

/// Handler for ATP burning
pub struct BurnHandler {
    event_store: EventStore,
    projection: ProjectionService,
    pool: PgPool,
}

impl BurnHandler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            event_store: EventStore::new(pool.clone()),
            projection: ProjectionService::new(pool.clone()),
            pool,
        }
    }

    /// Execute the burn command
    pub async fn execute(
        &self,
        command: BurnCommand,
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<BurnResult, AppError> {
        // Parse and validate amount
        let amount: Amount = command
            .amount
            .parse()
            .map_err(|e| AppError::InvalidRequest(format!("Invalid amount: {}", e)))?;

        // Get SYSTEM_BURN account
        let system_burn_user_id: Uuid = SYSTEM_BURN_USER_ID
            .parse()
            .expect("Invalid SYSTEM_BURN_USER_ID");

        let burn_account_id = self.get_system_account_id(system_burn_user_id).await?;

        // Get user's wallet account
        let from_account_id = self.get_wallet_account_id(command.from_user_id).await?;

        // Load user's account (use event sourcing if available, fallback to DB)
        let from_account = self.load_account_with_fallback(from_account_id).await?;

        // Load SYSTEM_BURN account from DB (bypasses event sourcing for system accounts)
        let burn_account = self.load_system_account(burn_account_id).await?;

        // Generate burn ID
        let burn_id = Uuid::new_v4();

        // Generate debit event from user
        let debit_description = format!("Burn: {}", command.reason);
        let debit_event = from_account.debit(&amount, burn_id, debit_description)?;

        // Generate credit event to SYSTEM_BURN
        let credit_description = format!("Burned from user: {}", command.reason);
        let credit_event = burn_account.credit(&amount, burn_id, credit_description)?;

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
                burn_account_id,
                burn_account.version(),
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

        // Update projections
        self.projection
            .apply_transfer(
                burn_id,
                event_ids[0],
                from_account_id,
                burn_account_id,
                &amount,
                from_account.version() + 1,
            )
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Apply events to get updated accounts
        let from_account = from_account.apply(debit_event);
        let burn_account = burn_account.apply(credit_event);

        // Save snapshots if needed
        self.event_store
            .save_snapshot_if_needed(&from_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.event_store
            .save_snapshot_if_needed(&burn_account)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(BurnResult {
            burn_id,
            from_user_id: command.from_user_id,
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
            .ok_or_else(|| AppError::Internal("System account not found".to_string()))?;

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
    fn test_burn_command() {
        let cmd = BurnCommand::new(
            Uuid::new_v4(),
            "100.00".to_string(),
            "Refund processing".to_string(),
        );

        assert_eq!(cmd.amount, "100.00");
        assert_eq!(cmd.reason, "Refund processing");
    }

    #[test]
    fn test_system_burn_user_id() {
        let id: Uuid = SYSTEM_BURN_USER_ID.parse().unwrap();
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000002");
    }
}

//! Projection Service
//!
//! Updates read-model tables from events.
//! This is the "P" in CQRS - projections for queries.

use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::Amount;

/// Projection Service for updating read models
#[derive(Debug, Clone)]
pub struct ProjectionService {
    pool: PgPool,
}

impl ProjectionService {
    /// Create a new ProjectionService
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // M087: apply_transfer
    // =========================================================================

    /// Apply a transfer to projections (account_balances + ledger_entries)
    /// This is called after events are persisted
    pub async fn apply_transfer(
        &self,
        transfer_id: Uuid,
        event_id: Uuid,
        from_account_id: Uuid,
        to_account_id: Uuid,
        amount: &Amount,
        event_version: i64,
    ) -> Result<(), ProjectionError> {
        let mut tx = self.pool.begin().await?;

        // M088: Update account_balances
        self.update_balance(&mut tx, from_account_id, amount, false, event_id, event_version)
            .await?;
        self.update_balance(&mut tx, to_account_id, amount, true, event_id, event_version)
            .await?;

        // M089: Create ledger entries (double-entry bookkeeping)
        self.create_ledger_entries(&mut tx, transfer_id, event_id, from_account_id, to_account_id, amount)
            .await?;

        tx.commit().await?;

        tracing::debug!(
            "Projection updated for transfer {}: {} -> {} ({})",
            transfer_id,
            from_account_id,
            to_account_id,
            amount
        );

        Ok(())
    }

    // =========================================================================
    // M088: update_balance
    // =========================================================================

    /// Update account balance (debit or credit)
    async fn update_balance(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        amount: &Amount,
        is_credit: bool,
        event_id: Uuid,
        event_version: i64,
    ) -> Result<(), ProjectionError> {
        let amount_value = amount.value();
        
        // Credit adds to balance, debit subtracts
        let balance_change = if is_credit {
            amount_value
        } else {
            -amount_value
        };

        let rows_affected = sqlx::query(
            r#"
            UPDATE account_balances
            SET 
                balance = balance + $2,
                last_event_id = $3,
                last_event_version = $4,
                updated_at = NOW()
            WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .bind(balance_change)
        .bind(event_id)
        .bind(event_version)
        .execute(&mut **tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            // Account balance record doesn't exist - create it
            sqlx::query(
                r#"
                INSERT INTO account_balances (account_id, balance, last_event_id, last_event_version)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(account_id)
            .bind(balance_change)
            .bind(event_id)
            .bind(event_version)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    // =========================================================================
    // M089: create_ledger_entries
    // =========================================================================

    /// Create double-entry bookkeeping ledger entries
    async fn create_ledger_entries(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        transfer_id: Uuid,
        event_id: Uuid,
        from_account_id: Uuid,
        to_account_id: Uuid,
        amount: &Amount,
    ) -> Result<(), ProjectionError> {
        let journal_id = transfer_id; // Use transfer_id as journal_id for simplicity
        let amount_value = amount.value();

        // Debit entry (money going TO recipient)
        sqlx::query(
            r#"
            INSERT INTO ledger_entries (journal_id, transfer_event_id, account_id, amount, entry_type)
            VALUES ($1, $2, $3, $4, 'debit')
            "#,
        )
        .bind(journal_id)
        .bind(event_id)
        .bind(to_account_id)
        .bind(amount_value)
        .execute(&mut **tx)
        .await?;

        // Credit entry (money going FROM sender)
        sqlx::query(
            r#"
            INSERT INTO ledger_entries (journal_id, transfer_event_id, account_id, amount, entry_type)
            VALUES ($1, $2, $3, $4, 'credit')
            "#,
        )
        .bind(journal_id)
        .bind(event_id)
        .bind(from_account_id)
        .bind(amount_value)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Create initial balance record for a new account
    pub async fn create_account_balance(
        &self,
        account_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), ProjectionError> {
        sqlx::query(
            r#"
            INSERT INTO account_balances (account_id, balance, last_event_id, last_event_version)
            VALUES ($1, 0, $2, 1)
            ON CONFLICT (account_id) DO NOTHING
            "#,
        )
        .bind(account_id)
        .bind(event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Apply a mint operation (ATP creation)
    pub async fn apply_mint(
        &self,
        transfer_id: Uuid,
        event_id: Uuid,
        mint_source_account_id: Uuid,
        recipient_account_id: Uuid,
        amount: &Amount,
        event_version: i64,
    ) -> Result<(), ProjectionError> {
        let mut tx = self.pool.begin().await?;

        // For mint: mint_source balance goes negative, recipient goes positive
        // This is valid for system accounts (mint_source can be negative)
        self.update_mint_source_balance(&mut tx, mint_source_account_id, amount, event_id, event_version)
            .await?;
        self.update_balance(&mut tx, recipient_account_id, amount, true, event_id, event_version)
            .await?;

        // Create ledger entries
        self.create_ledger_entries(&mut tx, transfer_id, event_id, mint_source_account_id, recipient_account_id, amount)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Update mint source balance (can go negative for liability accounts)
    async fn update_mint_source_balance(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        amount: &Amount,
        event_id: Uuid,
        event_version: i64,
    ) -> Result<(), ProjectionError> {
        // For liability accounts (mint_source), credit increases the balance (in accounting terms)
        // We track this as a negative number to represent liability
        let balance_change = -amount.value();

        sqlx::query(
            r#"
            UPDATE account_balances
            SET 
                balance = balance + $2,
                last_event_id = $3,
                last_event_version = $4,
                updated_at = NOW()
            WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .bind(balance_change)
        .bind(event_id)
        .bind(event_version)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Get current balance for an account
    pub async fn get_balance(&self, account_id: Uuid) -> Result<Decimal, ProjectionError> {
        let balance: Option<Decimal> = sqlx::query_scalar(
            r#"
            SELECT balance FROM account_balances WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(balance.unwrap_or(Decimal::ZERO))
    }

    /// Get balance for a user (by user_id, resolves to wallet account)
    pub async fn get_user_balance(&self, user_id: Uuid) -> Result<Option<Decimal>, ProjectionError> {
        let balance: Option<Decimal> = sqlx::query_scalar(
            r#"
            SELECT ab.balance 
            FROM account_balances ab
            JOIN accounts a ON ab.account_id = a.id
            WHERE a.user_id = $1 AND a.account_type = 'user_wallet'
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(balance)
    }
}

/// Projection errors
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Account not found: {0}")]
    AccountNotFound(Uuid),

    #[error("Insufficient balance")]
    InsufficientBalance,
}

// =========================================================================
// M090: Unit tests (Integration tests require database)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projection_error_display() {
        let err = ProjectionError::AccountNotFound(Uuid::nil());
        assert!(err.to_string().contains("Account not found"));

        let err = ProjectionError::InsufficientBalance;
        assert_eq!(err.to_string(), "Insufficient balance");
    }
}

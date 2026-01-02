//! Account Aggregate
//!
//! Account is the core aggregate for managing ATP balances.
//! It applies events to maintain current state and generates events for commands.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AccountEvent, Amount, Balance};
use crate::error::AppError;

use super::Aggregate;

/// Account status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Active,
    Frozen,
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Account Aggregate
/// 
/// Represents an ATP account with balance management.
/// State is derived from events, never directly mutated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Unique account ID
    id: Uuid,
    
    /// Owner user ID
    user_id: Uuid,
    
    /// Account type (user_wallet, mint_source, etc.)
    account_type: String,
    
    /// Current balance (derived from events)
    balance: Balance,
    
    /// Account status
    status: AccountStatus,
    
    /// Current version (number of events applied)
    version: i64,
    
    /// When the account was created
    created_at: Option<DateTime<Utc>>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            account_type: String::new(),
            balance: Balance::zero(),
            status: AccountStatus::Active,
            version: 0,
            created_at: None,
        }
    }
}

impl Account {
    // =========================================================================
    // M063: Account::create()
    // =========================================================================
    
    /// Create a new account and generate the creation event
    pub fn create(
        account_id: Uuid,
        user_id: Uuid,
        account_type: String,
    ) -> (Self, AccountEvent) {
        let now = Utc::now();
        
        let event = AccountEvent::AccountCreated {
            account_id,
            user_id,
            account_type: account_type.clone(),
            created_at: now,
        };
        
        let account = Self {
            id: account_id,
            user_id,
            account_type,
            balance: Balance::zero(),
            status: AccountStatus::Active,
            version: 1,
            created_at: Some(now),
        };
        
        (account, event)
    }

    /// Create an account from database state (bypasses event sourcing)
    /// Used for system accounts that are seeded directly in DB
    pub fn from_db_state(
        id: Uuid,
        user_id: Uuid,
        account_type: String,
        balance: rust_decimal::Decimal,
        version: i64,
    ) -> Self {
        // System accounts (like SYSTEM_MINT) can have negative balances
        // Use Balance::zero() if the value is negative, then set internal value directly
        let balance_value = if balance >= rust_decimal::Decimal::ZERO {
            Balance::new(balance).unwrap_or_else(|_| Balance::zero())
        } else {
            // For negative balances (SYSTEM_MINT liability), use internal construction
            Balance::from_decimal_unchecked(balance)
        };
        
        Self {
            id,
            user_id,
            account_type,
            balance: balance_value,
            status: AccountStatus::Active,
            version,
            created_at: None, // Not tracked for DB-loaded accounts
        }
    }

    // =========================================================================
    // M065: Account::debit()
    // =========================================================================
    
    /// Debit (withdraw) money from the account
    /// Returns the event to be persisted, or an error if not allowed
    pub fn debit(
        &self,
        amount: &Amount,
        transfer_id: Uuid,
        description: String,
    ) -> Result<AccountEvent, AppError> {
        // Check if account is frozen
        if self.status == AccountStatus::Frozen {
            return Err(AppError::AccountFrozen);
        }
        
        // Check if balance is sufficient
        if !self.balance.is_sufficient_for(amount) {
            return Err(AppError::InsufficientBalance);
        }
        
        Ok(AccountEvent::MoneyDebited {
            account_id: self.id,
            amount: amount.value(),
            transfer_id,
            description,
            debited_at: Utc::now(),
        })
    }

    // =========================================================================
    // M066: Account::credit()
    // =========================================================================
    
    /// Credit (deposit) money to the account
    /// Returns the event to be persisted, or an error if not allowed
    pub fn credit(
        &self,
        amount: &Amount,
        transfer_id: Uuid,
        description: String,
    ) -> Result<AccountEvent, AppError> {
        // Check if account is frozen
        if self.status == AccountStatus::Frozen {
            return Err(AppError::AccountFrozen);
        }
        
        Ok(AccountEvent::MoneyCredited {
            account_id: self.id,
            amount: amount.value(),
            transfer_id,
            description,
            credited_at: Utc::now(),
        })
    }

    /// Freeze the account
    pub fn freeze(&self, reason: String) -> Result<AccountEvent, AppError> {
        if self.status == AccountStatus::Frozen {
            return Err(AppError::InvalidRequest("Account is already frozen".to_string()));
        }
        
        Ok(AccountEvent::AccountFrozen {
            account_id: self.id,
            reason,
            frozen_at: Utc::now(),
        })
    }

    /// Unfreeze the account
    pub fn unfreeze(&self) -> Result<AccountEvent, AppError> {
        if self.status != AccountStatus::Frozen {
            return Err(AppError::InvalidRequest("Account is not frozen".to_string()));
        }
        
        Ok(AccountEvent::AccountUnfrozen {
            account_id: self.id,
            unfrozen_at: Utc::now(),
        })
    }

    // =========================================================================
    // Getters
    // =========================================================================
    
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }
    
    pub fn account_type(&self) -> &str {
        &self.account_type
    }
    
    pub fn balance(&self) -> &Balance {
        &self.balance
    }
    
    pub fn status(&self) -> &AccountStatus {
        &self.status
    }
    
    pub fn is_frozen(&self) -> bool {
        self.status == AccountStatus::Frozen
    }
    
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.created_at
    }
}

// =========================================================================
// M064: Account::apply()
// =========================================================================

impl Aggregate for Account {
    type Event = AccountEvent;

    fn aggregate_type() -> &'static str {
        "Account"
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn version(&self) -> i64 {
        self.version
    }

    fn apply(mut self, event: Self::Event) -> Self {
        match event {
            AccountEvent::AccountCreated {
                account_id,
                user_id,
                account_type,
                created_at,
            } => {
                self.id = account_id;
                self.user_id = user_id;
                self.account_type = account_type;
                self.balance = Balance::zero();
                self.status = AccountStatus::Active;
                self.created_at = Some(created_at);
            }
            
            AccountEvent::MoneyCredited { amount, .. } => {
                // Safely handle invalid amount in event
                match Amount::new(amount) {
                    Ok(amt) => {
                        match self.balance.credit(&amt) {
                            Ok(new_balance) => self.balance = new_balance,
                            Err(e) => {
                                tracing::error!(
                                    "Balance overflow during credit replay for account {}: {}",
                                    self.id, e
                                );
                                // Keep current balance to maintain consistency
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Invalid amount in MoneyCredited event for account {}: {}",
                            self.id, e
                        );
                    }
                }
            }
            
            AccountEvent::MoneyDebited { amount, .. } => {
                // Safely handle invalid amount in event
                match Amount::new(amount) {
                    Ok(amt) => {
                        match self.balance.debit(&amt) {
                            Ok(new_balance) => self.balance = new_balance,
                            Err(e) => {
                                tracing::error!(
                                    "Balance underflow during debit replay for account {}: {}",
                                    self.id, e
                                );
                                // Keep current balance to maintain consistency
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Invalid amount in MoneyDebited event for account {}: {}",
                            self.id, e
                        );
                    }
                }
            }
            
            AccountEvent::AccountFrozen { .. } => {
                self.status = AccountStatus::Frozen;
            }
            
            AccountEvent::AccountUnfrozen { .. } => {
                self.status = AccountStatus::Active;
            }
        }
        
        self.version += 1;
        self
    }
    
    // M067: Account::should_snapshot() - inherited from trait default
}

// =========================================================================
// M068: Account unit tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_account_create() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        
        let (account, event) = Account::create(
            account_id,
            user_id,
            "user_wallet".to_string(),
        );
        
        assert_eq!(account.id(), account_id);
        assert_eq!(account.user_id(), user_id);
        assert_eq!(account.account_type(), "user_wallet");
        assert_eq!(account.balance().value(), Decimal::ZERO);
        assert_eq!(account.version(), 1);
        assert!(matches!(event, AccountEvent::AccountCreated { .. }));
    }

    #[test]
    fn test_account_credit() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        let amount = Amount::new(Decimal::new(100, 0)).unwrap();
        let transfer_id = Uuid::new_v4();
        
        let event = account.credit(&amount, transfer_id, "Test credit".to_string()).unwrap();
        
        assert!(matches!(event, AccountEvent::MoneyCredited { .. }));
        
        // Apply event
        let account = account.apply(event);
        assert_eq!(account.balance().value(), Decimal::new(100, 0));
        assert_eq!(account.version(), 2);
    }

    #[test]
    fn test_account_debit() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        // First credit some money
        let credit_amount = Amount::new(Decimal::new(100, 0)).unwrap();
        let credit_event = account.credit(&credit_amount, Uuid::new_v4(), "Credit".to_string()).unwrap();
        let account = account.apply(credit_event);
        
        // Then debit
        let debit_amount = Amount::new(Decimal::new(30, 0)).unwrap();
        let debit_event = account.debit(&debit_amount, Uuid::new_v4(), "Debit".to_string()).unwrap();
        let account = account.apply(debit_event);
        
        assert_eq!(account.balance().value(), Decimal::new(70, 0));
        assert_eq!(account.version(), 3);
    }

    #[test]
    fn test_account_insufficient_balance() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        let amount = Amount::new(Decimal::new(100, 0)).unwrap();
        let result = account.debit(&amount, Uuid::new_v4(), "Too much".to_string());
        
        assert!(matches!(result, Err(AppError::InsufficientBalance)));
    }

    #[test]
    fn test_account_frozen() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        // Freeze account
        let freeze_event = account.freeze("Suspicious activity".to_string()).unwrap();
        let account = account.apply(freeze_event);
        
        assert!(account.is_frozen());
        
        // Try to credit - should fail
        let amount = Amount::new(Decimal::new(100, 0)).unwrap();
        let result = account.credit(&amount, Uuid::new_v4(), "Credit".to_string());
        assert!(matches!(result, Err(AppError::AccountFrozen)));
        
        // Try to debit - should fail
        let result = account.debit(&amount, Uuid::new_v4(), "Debit".to_string());
        assert!(matches!(result, Err(AppError::AccountFrozen)));
    }

    #[test]
    fn test_account_unfreeze() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        // Freeze then unfreeze
        let freeze_event = account.freeze("Test".to_string()).unwrap();
        let account = account.apply(freeze_event);
        
        let unfreeze_event = account.unfreeze().unwrap();
        let account = account.apply(unfreeze_event);
        
        assert!(!account.is_frozen());
        assert_eq!(account.status(), &AccountStatus::Active);
    }

    #[test]
    fn test_should_snapshot() {
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let (mut account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        
        // Version 1 - no snapshot
        assert!(!account.should_snapshot());
        
        // Simulate applying events to reach version 100
        account.version = 100;
        assert!(account.should_snapshot());
        
        account.version = 99;
        assert!(!account.should_snapshot());
        
        account.version = 200;
        assert!(account.should_snapshot());
    }
}

//! Domain Events
//!
//! Event definitions for Event Sourcing.
//! Events are immutable facts that have happened in the system.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Account-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AccountEvent {
    /// Account was created
    AccountCreated {
        account_id: Uuid,
        user_id: Uuid,
        account_type: String,
        created_at: DateTime<Utc>,
    },

    /// Money was credited to the account (balance increased)
    MoneyCredited {
        account_id: Uuid,
        amount: Decimal,
        transfer_id: Uuid,
        description: String,
        credited_at: DateTime<Utc>,
    },

    /// Money was debited from the account (balance decreased)
    MoneyDebited {
        account_id: Uuid,
        amount: Decimal,
        transfer_id: Uuid,
        description: String,
        debited_at: DateTime<Utc>,
    },

    /// Account was frozen
    AccountFrozen {
        account_id: Uuid,
        reason: String,
        frozen_at: DateTime<Utc>,
    },

    /// Account was unfrozen
    AccountUnfrozen {
        account_id: Uuid,
        unfrozen_at: DateTime<Utc>,
    },
}

impl AccountEvent {
    /// Get the event type as a string
    pub fn event_type(&self) -> &'static str {
        match self {
            AccountEvent::AccountCreated { .. } => "AccountCreated",
            AccountEvent::MoneyCredited { .. } => "MoneyCredited",
            AccountEvent::MoneyDebited { .. } => "MoneyDebited",
            AccountEvent::AccountFrozen { .. } => "AccountFrozen",
            AccountEvent::AccountUnfrozen { .. } => "AccountUnfrozen",
        }
    }

    /// Get the account ID this event relates to
    pub fn account_id(&self) -> Uuid {
        match self {
            AccountEvent::AccountCreated { account_id, .. } => *account_id,
            AccountEvent::MoneyCredited { account_id, .. } => *account_id,
            AccountEvent::MoneyDebited { account_id, .. } => *account_id,
            AccountEvent::AccountFrozen { account_id, .. } => *account_id,
            AccountEvent::AccountUnfrozen { account_id, .. } => *account_id,
        }
    }
}

/// Transfer-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TransferEvent {
    /// Transfer was initiated
    TransferInitiated {
        transfer_id: Uuid,
        from_account_id: Uuid,
        to_account_id: Uuid,
        from_user_id: Uuid,
        to_user_id: Uuid,
        amount: Decimal,
        memo: Option<String>,
        initiated_by: Uuid,
        initiated_at: DateTime<Utc>,
    },

    /// Transfer was completed successfully
    TransferCompleted {
        transfer_id: Uuid,
        completed_at: DateTime<Utc>,
    },

    /// Transfer failed
    TransferFailed {
        transfer_id: Uuid,
        reason: TransferFailureReason,
        failed_at: DateTime<Utc>,
    },
}

impl TransferEvent {
    /// Get the event type as a string
    pub fn event_type(&self) -> &'static str {
        match self {
            TransferEvent::TransferInitiated { .. } => "TransferInitiated",
            TransferEvent::TransferCompleted { .. } => "TransferCompleted",
            TransferEvent::TransferFailed { .. } => "TransferFailed",
        }
    }

    /// Get the transfer ID this event relates to
    pub fn transfer_id(&self) -> Uuid {
        match self {
            TransferEvent::TransferInitiated { transfer_id, .. } => *transfer_id,
            TransferEvent::TransferCompleted { transfer_id, .. } => *transfer_id,
            TransferEvent::TransferFailed { transfer_id, .. } => *transfer_id,
        }
    }
}

/// Reasons why a transfer might fail
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferFailureReason {
    /// Sender doesn't have enough balance
    InsufficientBalance,

    /// Sender's account is frozen
    AccountFrozen,

    /// Account not found
    AccountNotFound,

    /// Cannot transfer to the same account
    SameAccount,

    /// Amount is too small (below minimum)
    AmountTooSmall,

    /// Amount is too large (exceeds maximum)
    AmountTooLarge,

    /// Request user doesn't match account owner
    UnauthorizedTransfer,

    /// Concurrent modification detected
    ConcurrencyConflict,

    /// Internal system error
    InternalError,
}

impl std::fmt::Display for TransferFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferFailureReason::InsufficientBalance => write!(f, "Insufficient balance"),
            TransferFailureReason::AccountFrozen => write!(f, "Account is frozen"),
            TransferFailureReason::AccountNotFound => write!(f, "Account not found"),
            TransferFailureReason::SameAccount => write!(f, "Cannot transfer to same account"),
            TransferFailureReason::AmountTooSmall => write!(f, "Amount is too small"),
            TransferFailureReason::AmountTooLarge => write!(f, "Amount is too large"),
            TransferFailureReason::UnauthorizedTransfer => write!(f, "Unauthorized transfer"),
            TransferFailureReason::ConcurrencyConflict => write!(f, "Concurrency conflict"),
            TransferFailureReason::InternalError => write!(f, "Internal error"),
        }
    }
}

/// User-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UserEvent {
    /// User was created
    UserCreated {
        user_id: Uuid,
        username: String,
        email: String,
        display_name: Option<String>,
        created_at: DateTime<Utc>,
    },

    /// User profile was updated
    UserUpdated {
        user_id: Uuid,
        changes: UserChanges,
        updated_at: DateTime<Utc>,
    },

    /// User was deactivated (soft delete)
    UserDeactivated {
        user_id: Uuid,
        reason: Option<String>,
        deactivated_at: DateTime<Utc>,
    },

    /// User was reactivated
    UserReactivated {
        user_id: Uuid,
        reactivated_at: DateTime<Utc>,
    },
}

/// Changes made to a user profile
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl UserEvent {
    /// Get the event type as a string
    pub fn event_type(&self) -> &'static str {
        match self {
            UserEvent::UserCreated { .. } => "UserCreated",
            UserEvent::UserUpdated { .. } => "UserUpdated",
            UserEvent::UserDeactivated { .. } => "UserDeactivated",
            UserEvent::UserReactivated { .. } => "UserReactivated",
        }
    }

    /// Get the user ID this event relates to
    pub fn user_id(&self) -> Uuid {
        match self {
            UserEvent::UserCreated { user_id, .. } => *user_id,
            UserEvent::UserUpdated { user_id, .. } => *user_id,
            UserEvent::UserDeactivated { user_id, .. } => *user_id,
            UserEvent::UserReactivated { user_id, .. } => *user_id,
        }
    }
}

/// A generic domain event wrapper for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: i64,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_event_serialization() {
        let event = AccountEvent::MoneyCredited {
            account_id: Uuid::new_v4(),
            amount: Decimal::new(100, 0),
            transfer_id: Uuid::new_v4(),
            description: "Test credit".to_string(),
            credited_at: Utc::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("MoneyCredited"));
        
        let deserialized: AccountEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.event_type(), deserialized.event_type());
    }

    #[test]
    fn test_transfer_failure_reason() {
        let reason = TransferFailureReason::InsufficientBalance;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, r#""insufficient_balance""#);
        
        let deserialized: TransferFailureReason = serde_json::from_str(&json).unwrap();
        assert_eq!(reason, deserialized);
    }
}

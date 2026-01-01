//! Domain Error Types
//!
//! Pure domain errors that don't depend on infrastructure.

use thiserror::Error;

/// M132: Domain-specific errors
/// 
/// These errors represent business rule violations and domain invariant failures.
/// They are independent of the web/infrastructure layer.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum DomainError {
    /// Insufficient balance for debit operation
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance {
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    /// Account is frozen and cannot process transactions
    #[error("Account is frozen: {reason}")]
    AccountFrozen { reason: String },

    /// Account is not active
    #[error("Account is not active")]
    AccountNotActive,

    /// Invalid amount (zero, negative, or exceeds limit)
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    /// User not found
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// Account not found
    #[error("Account not found: {0}")]
    AccountNotFound(String),

    /// Transfer to same account
    #[error("Cannot transfer to the same account")]
    SameAccountTransfer,

    /// Unauthorized operation
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Business rule violation
    #[error("Business rule violation: {0}")]
    BusinessRuleViolation(String),

    /// Aggregate version conflict (optimistic locking)
    #[error("Version conflict: expected {expected}, found {found}")]
    VersionConflict { expected: i64, found: i64 },

    /// Duplicate operation (idempotency)
    #[error("Duplicate operation: {key}")]
    DuplicateOperation { key: String },
}

impl DomainError {
    /// Create an insufficient balance error
    pub fn insufficient_balance(
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    ) -> Self {
        Self::InsufficientBalance { required, available }
    }

    /// Create an account frozen error
    pub fn account_frozen(reason: impl Into<String>) -> Self {
        Self::AccountFrozen {
            reason: reason.into(),
        }
    }

    /// Check if this is a client error (user's fault)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::InsufficientBalance { .. }
                | Self::AccountFrozen { .. }
                | Self::AccountNotActive
                | Self::InvalidAmount(_)
                | Self::SameAccountTransfer
                | Self::Unauthorized(_)
                | Self::BusinessRuleViolation(_)
        )
    }

    /// Check if this is a conflict error (retry may help)
    pub fn is_conflict_error(&self) -> bool {
        matches!(
            self,
            Self::VersionConflict { .. } | Self::DuplicateOperation { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_insufficient_balance_error() {
        let err = DomainError::insufficient_balance(
            Decimal::new(100, 0),
            Decimal::new(50, 0),
        );
        
        assert!(err.is_client_error());
        assert!(!err.is_conflict_error());
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("50"));
    }

    #[test]
    fn test_account_frozen_error() {
        let err = DomainError::account_frozen("Suspicious activity");
        
        assert!(err.is_client_error());
        assert!(err.to_string().contains("Suspicious activity"));
    }

    #[test]
    fn test_version_conflict_error() {
        let err = DomainError::VersionConflict {
            expected: 1,
            found: 2,
        };
        
        assert!(!err.is_client_error());
        assert!(err.is_conflict_error());
    }
}

//! Command definitions
//!
//! Commands represent intentions to change the system state.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =========================================================================
// M097: CreateUserCommand
// =========================================================================

/// Command to create a new user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserCommand {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
}

impl CreateUserCommand {
    pub fn new(user_id: Uuid, username: String, email: String) -> Self {
        Self {
            user_id,
            username,
            email,
            display_name: None,
        }
    }

    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }
}

// =========================================================================
// M101: TransferCommand
// =========================================================================

/// Command to transfer ATP between users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferCommand {
    /// User ID of the sender (resolved to account internally)
    pub from_user_id: Uuid,
    /// User ID of the recipient (resolved to account internally)
    pub to_user_id: Uuid,
    /// Amount to transfer (as string for precise decimal)
    pub amount: String,
    /// Optional memo
    pub memo: Option<String>,
}

impl TransferCommand {
    pub fn new(from_user_id: Uuid, to_user_id: Uuid, amount: String) -> Self {
        Self {
            from_user_id,
            to_user_id,
            amount,
            memo: None,
        }
    }

    pub fn with_memo(mut self, memo: String) -> Self {
        self.memo = Some(memo);
        self
    }
}

// =========================================================================
// M108: MintCommand
// =========================================================================

/// Command to mint (create) new ATP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintCommand {
    /// User ID to receive minted ATP
    pub recipient_user_id: Uuid,
    /// Amount to mint (as string for precise decimal)
    pub amount: String,
    /// Reason for minting
    pub reason: String,
}

impl MintCommand {
    pub fn new(recipient_user_id: Uuid, amount: String, reason: String) -> Self {
        Self {
            recipient_user_id,
            amount,
            reason,
        }
    }
}

/// Result of a successful transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResult {
    pub transfer_id: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub amount: Decimal,
    pub status: String,
}

/// Result of a successful mint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintResult {
    pub mint_id: Uuid,
    pub recipient_user_id: Uuid,
    pub amount: Decimal,
}

/// Result of a successful user creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserResult {
    pub user_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
}

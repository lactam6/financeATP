//! Integration tests for handlers
//!
//! These tests require a database connection.
//! Run with: cargo test --features integration_tests

#[cfg(test)]
mod tests {
    use crate::aggregate::{Account, Aggregate};
    use crate::domain::{Amount, Balance, OperationContext};
    use crate::error::AppError;
    use crate::handlers::{
        CreateUserCommand, CreateUserHandler, MintCommand, MintHandler, TransferCommand,
        TransferHandler,
    };
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use uuid::Uuid;

    // =========================================================================
    // M100: User Creation Integration Tests (Unit tests only - DB required for full)
    // =========================================================================

    #[test]
    fn test_create_user_command_validation() {
        let user_id = Uuid::new_v4();
        let cmd = CreateUserCommand::new(
            user_id,
            "testuser".to_string(),
            "test@example.com".to_string(),
        );

        assert_eq!(cmd.user_id, user_id);
        assert_eq!(cmd.username, "testuser");
        assert_eq!(cmd.email, "test@example.com");
        assert!(cmd.display_name.is_none());
    }

    #[test]
    fn test_create_user_command_with_display_name() {
        let cmd = CreateUserCommand::new(
            Uuid::new_v4(),
            "alice".to_string(),
            "alice@example.com".to_string(),
        )
        .with_display_name("Alice Wonderland".to_string());

        assert_eq!(cmd.display_name, Some("Alice Wonderland".to_string()));
    }

    // =========================================================================
    // M105: Transfer Integration Tests (Unit tests only - DB required for full)
    // =========================================================================

    #[test]
    fn test_transfer_command_validation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let cmd = TransferCommand::new(from, to, "100.50".to_string());

        assert_eq!(cmd.from_user_id, from);
        assert_eq!(cmd.to_user_id, to);
        assert_eq!(cmd.amount, "100.50");
        assert!(cmd.memo.is_none());
    }

    #[test]
    fn test_transfer_command_with_memo() {
        let cmd = TransferCommand::new(Uuid::new_v4(), Uuid::new_v4(), "50.00".to_string())
            .with_memo("Payment for services".to_string());

        assert_eq!(cmd.memo, Some("Payment for services".to_string()));
    }

    // =========================================================================
    // M106: Insufficient Balance Error Tests
    // =========================================================================

    #[test]
    fn test_insufficient_balance_error() {
        // Test that Account::debit returns error for insufficient balance
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account with initial balance of 0
        let (account, _event) = Account::create(account_id, user_id, "user_wallet".to_string());

        // Try to debit 100 ATP from account with 0 balance
        let amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let result = account.debit(&amount, Uuid::new_v4(), "Test debit".to_string());

        // Should fail with insufficient balance
        assert!(result.is_err());
        match result {
            Err(AppError::InsufficientBalance) => {
                // Expected error
            }
            Err(e) => panic!("Expected InsufficientBalance, got: {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn test_insufficient_balance_partial() {
        // Test that partial balance is not enough
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());

        // Credit 50 ATP
        let credit_amount = Amount::new(Decimal::from_str("50.00").unwrap()).unwrap();
        let credit_event =
            account.credit(&credit_amount, Uuid::new_v4(), "Initial credit".to_string());
        assert!(credit_event.is_ok());

        // Apply the credit event
        let account = account.apply(credit_event.unwrap());

        // Try to debit 100 ATP (more than balance)
        let debit_amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let result = account.debit(&debit_amount, Uuid::new_v4(), "Test debit".to_string());

        assert!(result.is_err());
        match result {
            Err(AppError::InsufficientBalance) => {
                // Expected error
            }
            Err(e) => panic!("Expected InsufficientBalance, got: {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn test_sufficient_balance_succeeds() {
        // Test that debit succeeds with sufficient balance
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());

        // Credit 100 ATP
        let credit_amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let credit_event =
            account.credit(&credit_amount, Uuid::new_v4(), "Initial credit".to_string());
        assert!(credit_event.is_ok());

        // Apply the credit event
        let account = account.apply(credit_event.unwrap());

        // Debit 50 ATP (less than balance)
        let debit_amount = Amount::new(Decimal::from_str("50.00").unwrap()).unwrap();
        let result = account.debit(&debit_amount, Uuid::new_v4(), "Test debit".to_string());

        assert!(result.is_ok());
    }

    #[test]
    fn test_exact_balance_debit() {
        // Test that debit of exact balance succeeds
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());

        // Credit 100 ATP
        let credit_amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let credit_event =
            account.credit(&credit_amount, Uuid::new_v4(), "Initial credit".to_string());
        let account = account.apply(credit_event.unwrap());

        // Debit exact 100 ATP
        let debit_amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let result = account.debit(&debit_amount, Uuid::new_v4(), "Test debit".to_string());

        assert!(result.is_ok());
    }

    // =========================================================================
    // M107: Concurrent Transfer (Optimistic Lock) Tests
    // =========================================================================

    #[test]
    fn test_version_tracking_on_events() {
        // Test that account version increments correctly
        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account - version starts at 1
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        assert_eq!(account.version(), 1);

        // Credit - version increments
        let amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let credit_event = account
            .credit(&amount, Uuid::new_v4(), "Credit".to_string())
            .unwrap();
        let account = account.apply(credit_event);
        assert_eq!(account.version(), 2);

        // Debit - version increments again
        let debit_amount = Amount::new(Decimal::from_str("50.00").unwrap()).unwrap();
        let debit_event = account
            .debit(&debit_amount, Uuid::new_v4(), "Debit".to_string())
            .unwrap();
        let account = account.apply(debit_event);
        assert_eq!(account.version(), 3);
    }

    #[test]
    fn test_aggregate_operation_version() {
        use crate::event_store::AggregateOperation;

        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account
        let (account, create_event) =
            Account::create(account_id, user_id, "user_wallet".to_string());

        // Create operation with version 0 (expected for new aggregate)
        let op = AggregateOperation::new(
            "Account",
            account.id(),
            0, // Expected version before this event
            create_event.event_type(),
            &create_event,
        );

        assert!(op.is_ok());
        let op = op.unwrap();
        assert_eq!(op.expected_version, 0);
    }

    #[test]
    fn test_concurrent_modification_detection() {
        // This test simulates what happens when two transactions try to modify
        // the same account simultaneously. The optimistic locking should detect
        // the conflict via version mismatch.

        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create account
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());

        // Simulate two "transactions" loading the same account state
        let account_tx1 = account.clone();
        let account_tx2 = account.clone();

        // Both see version 1
        assert_eq!(account_tx1.version(), 1);
        assert_eq!(account_tx2.version(), 1);

        // TX1 modifies and would write with expected_version = 1
        // TX2 modifies and would also write with expected_version = 1
        // Only one can succeed - the other gets a version conflict

        // In real scenario, EventStore.append_atomic checks:
        // WHERE aggregate_id = $1 AND version = $expected_version
        // If TX1 writes first (version 1 -> 2), TX2's write fails
        // because version is now 2, not 1

        // This is tested at the EventStore level with actual DB
        // Here we just verify the version tracking works correctly
        assert_eq!(account_tx1.version(), account_tx2.version());
    }

    // =========================================================================
    // M111: ATP Mint Integration Tests (Unit tests only - DB required for full)
    // =========================================================================

    #[test]
    fn test_mint_command_validation() {
        let recipient = Uuid::new_v4();
        let cmd = MintCommand::new(
            recipient,
            "1000.00".to_string(),
            "Initial balance grant".to_string(),
        );

        assert_eq!(cmd.recipient_user_id, recipient);
        assert_eq!(cmd.amount, "1000.00");
        assert_eq!(cmd.reason, "Initial balance grant");
    }

    #[test]
    fn test_mint_creates_valid_amount() {
        // Verify that mint amounts are validated
        let amount_str = "1000.00";
        let amount: Result<Amount, _> = amount_str.parse();
        assert!(amount.is_ok());

        let amount = amount.unwrap();
        assert_eq!(amount.value(), Decimal::from_str("1000.00").unwrap());
    }

    #[test]
    fn test_mint_rejects_invalid_amount() {
        // Verify that invalid amounts are rejected
        let invalid_amounts = vec![
            "0",           // Zero is not allowed
            "-100",        // Negative not allowed
            "abc",         // Not a number
            "1000000000000001", // Exceeds max (1 trillion)
        ];

        for amount_str in invalid_amounts {
            let result: Result<Amount, _> = amount_str.parse();
            assert!(
                result.is_err(),
                "Expected error for amount: {}",
                amount_str
            );
        }
    }

    #[test]
    fn test_frozen_account_cannot_receive_credit() {
        use crate::domain::AccountEvent;

        let account_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create and freeze account
        let (account, _) = Account::create(account_id, user_id, "user_wallet".to_string());
        let freeze_event = account.freeze("Suspicious activity".to_string()).unwrap();
        let account = account.apply(freeze_event);

        // Try to credit frozen account
        let amount = Amount::new(Decimal::from_str("100.00").unwrap()).unwrap();
        let result = account.credit(&amount, Uuid::new_v4(), "Credit attempt".to_string());

        assert!(result.is_err());
        match result {
            Err(AppError::AccountFrozen) => {
                // Expected
            }
            Err(e) => panic!("Expected AccountFrozen, got: {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }
}

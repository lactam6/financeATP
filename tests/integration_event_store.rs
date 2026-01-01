//! Integration tests for Event Store (M155, M159)

use finance_atp::aggregate::Aggregate;
use finance_atp::domain::{AccountEvent, OperationContext};
use finance_atp::event_store::{EventStore, AggregateOperation};
use chrono::Utc;
use uuid::Uuid;

mod common;

#[tokio::test]
async fn test_event_store_append_and_load() {
    let pool = common::setup_test_db().await;
    let event_store = EventStore::new(pool);

    let account_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Create an AccountCreated event
    let event = AccountEvent::AccountCreated {
        account_id,
        user_id,
        account_type: "user_wallet".to_string(),
        created_at: Utc::now(),
    };

    let op = AggregateOperation::new(
        "Account",
        account_id,
        0, // expected version
        "AccountCreated",
        &event,
    ).unwrap();

    let context = OperationContext::new()
        .with_correlation_id(Uuid::new_v4());

    // Append event
    let event_ids = event_store.append_atomic(vec![op], None, &context).await.unwrap();
    assert_eq!(event_ids.len(), 1);

    // Load events
    let events = event_store.get_events(account_id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "AccountCreated");
    assert_eq!(events[0].version, 1);
}

#[tokio::test]
async fn test_event_store_concurrency_conflict() {
    let pool = common::setup_test_db().await;
    let event_store = EventStore::new(pool);

    let account_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Create first event
    let event1 = AccountEvent::AccountCreated {
        account_id,
        user_id,
        account_type: "user_wallet".to_string(),
        created_at: Utc::now(),
    };

    let op1 = AggregateOperation::new("Account", account_id, 0, "AccountCreated", &event1).unwrap();
    let context = OperationContext::new().with_correlation_id(Uuid::new_v4());

    event_store.append_atomic(vec![op1], None, &context).await.unwrap();

    // Try to append with wrong expected version (should fail)
    let event2 = AccountEvent::AccountFrozen {
        account_id,
        reason: "Test freeze".to_string(),
        frozen_at: Utc::now(),
    };

    let op2 = AggregateOperation::new("Account", account_id, 0, "AccountFrozen", &event2).unwrap(); // Wrong version!

    let result = event_store.append_atomic(vec![op2], None, &context).await;
    assert!(result.is_err(), "Should fail due to version conflict");
}

//! API Integration Tests (M156-M158)

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    middleware,
};
use tower::util::ServiceExt;
use finance_atp::api::{self, routes::{CreateUserRequest, MintRequest, TransferRequest}};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use serde_json::Value;

mod common;

#[tokio::test]
async fn test_transfer_e2e() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());
    let api_key = "test_key_123";

    // 1. Create User A
    let user_a_id = Uuid::new_v4();
    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_a_id,
            username: "user_a".to_string(),
            email: "user_a@example.com".to_string(),
            display_name: Some("User A".to_string()),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "User A creation failed");

    // 2. Create User B
    let user_b_id = Uuid::new_v4();
    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_b_id,
            username: "user_b".to_string(),
            email: "user_b@example.com".to_string(),
            display_name: Some("User B".to_string()),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "User B creation failed");

    // 3. Mint money to User A
    let req = Request::builder()
        .method("POST")
        .uri("/admin/mint")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&MintRequest {
            recipient_user_id: user_a_id,
            amount: "1000.00".to_string(),
            reason: "Initial mint".to_string(),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "Mint failed");

    // 4. Transfer from A to B
    let req = Request::builder()
        .method("POST")
        .uri("/transfers")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&TransferRequest {
            from_user_id: user_a_id,
            to_user_id: user_b_id,
            amount: "300.00".to_string(),
            memo: Some("Payment for goods".to_string()),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "Transfer failed");

    // 5. Verify User A balance
    let req = Request::builder()
        .method("GET")
        .uri(format!("/users/{}/balance", user_a_id))
        .header("X-API-Key", api_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["balance"], "700.00000000");

    // 6. Verify User B balance
    let req = Request::builder()
        .method("GET")
        .uri(format!("/users/{}/balance", user_b_id))
        .header("X-API-Key", api_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["balance"], "300.00000000");
}

#[tokio::test]
async fn test_idempotency_api() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());
    let api_key = "test_key_123";

    // Create user
    let user_id = Uuid::new_v4();
    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id,
            username: "idem_user".to_string(),
            email: "idem@test.com".to_string(),
            display_name: None,
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let idempotency_key = Uuid::new_v4();
    let mint_req = MintRequest {
        recipient_user_id: user_id,
        amount: "50.00".to_string(),
        reason: "Idempotent mint".to_string(),
    };

    // First Request
    let req = Request::builder()
        .method("POST")
        .uri("/admin/mint")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_id.to_string())
        .header("Idempotency-Key", idempotency_key.to_string())
        .body(Body::from(serde_json::to_string(&mint_req).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second Request (Same Idempotency Key)
    let req = Request::builder()
        .method("POST")
        .uri("/admin/mint")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_id.to_string())
        .header("Idempotency-Key", idempotency_key.to_string())
        .body(Body::from(serde_json::to_string(&mint_req).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED); // Idempotent - returns same result

    // Verify balance is 50, not 100 (idempotency worked)
    let req = Request::builder()
        .method("GET")
        .uri(format!("/users/{}/balance", user_id))
        .header("X-API-Key", api_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["balance"], "50.00000000", "Idempotency failed - balance should be 50");
}

#[tokio::test]
async fn test_account_history_last_month_newest_first() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());
    let api_key = "test_key_123";

    // Create users
    let user_a_id = Uuid::new_v4();
    let user_b_id = Uuid::new_v4();

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_a_id,
            username: "history_user_a".to_string(),
            email: "history_a@example.com".to_string(),
            display_name: None,
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_b_id,
            username: "history_user_b".to_string(),
            email: "history_b@example.com".to_string(),
            display_name: None,
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Mint to user A (creates MoneyCredited on A)
    let req = Request::builder()
        .method("POST")
        .uri("/admin/mint")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&MintRequest {
            recipient_user_id: user_a_id,
            amount: "1000.00".to_string(),
            reason: "History test mint".to_string(),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Transfer A -> B (creates MoneyDebited on A)
    let req = Request::builder()
        .method("POST")
        .uri("/transfers")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&TransferRequest {
            from_user_id: user_a_id,
            to_user_id: user_b_id,
            amount: "300.00".to_string(),
            memo: Some("History test transfer".to_string()),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Resolve user A account_id
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = $1 AND account_type = 'user_wallet' LIMIT 1",
    )
    .bind(user_a_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Insert an old event (>1 month) that should be filtered out
    let old_payload = serde_json::json!({
        "amount": "1.00000000",
        "description": "old_out_of_range_event"
    });
    sqlx::query(
        r#"
        INSERT INTO events (id, aggregate_type, aggregate_id, event_type, version, event_data, context, created_at)
        VALUES ($1, 'Account', $2, 'MoneyCredited', $3, $4, '{}', NOW() - INTERVAL '40 days')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(9999_i64)
    .bind(old_payload)
    .execute(&pool)
    .await
    .unwrap();

    // Call new endpoint
    let req = Request::builder()
        .method("GET")
        .uri(format!("/accounts/{}/history", account_id))
        .header("X-API-Key", api_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["account_id"], account_id.to_string());

    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "Expected at least one in-range transaction entry");

    // Out-of-range event should not be included
    assert!(
        entries.iter().all(|e| e["description"] != "old_out_of_range_event"),
        "Out-of-range event was included"
    );

    // Validate ordering (newest first) and date range (last month)
    let threshold = Utc::now() - Duration::days(31);
    let mut timestamps = Vec::new();
    for entry in entries {
        let created_at = entry["created_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap();
        assert!(
            created_at >= threshold,
            "Entry older than last month was included: {}",
            created_at
        );
        timestamps.push(created_at);
    }

    for pair in timestamps.windows(2) {
        assert!(
            pair[0] >= pair[1],
            "Entries are not sorted in descending created_at order"
        );
    }
}

#[tokio::test]
async fn test_account_history_requires_read_accounts_permission() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());

    // Seed a key without read:accounts permission
    let limited_key = "limited_key_no_accounts";
    let limited_hash: String = sqlx::query_scalar(
        "SELECT encode(sha256($1::bytea), 'hex')",
    )
    .bind(limited_key.as_bytes())
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO api_keys (id, name, key_hash, key_prefix, permissions, is_active)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind("Limited Key")
    .bind(limited_hash)
    .bind("lim_")
    .bind(vec!["read:users".to_string()])
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    // Any account_id is fine because permission check runs first
    let req = Request::builder()
        .method("GET")
        .uri(format!("/accounts/{}/history", Uuid::new_v4()))
        .header("X-API-Key", limited_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_user_history_applies_account_history_filters() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());
    let api_key = "test_key_123";

    let user_a_id = Uuid::new_v4();
    let user_b_id = Uuid::new_v4();

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_a_id,
            username: "user_history_a".to_string(),
            email: "user_history_a@example.com".to_string(),
            display_name: None,
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .body(Body::from(serde_json::to_string(&CreateUserRequest {
            user_id: user_b_id,
            username: "user_history_b".to_string(),
            email: "user_history_b@example.com".to_string(),
            display_name: None,
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("POST")
        .uri("/admin/mint")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&MintRequest {
            recipient_user_id: user_a_id,
            amount: "500.00".to_string(),
            reason: "User history mint".to_string(),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("POST")
        .uri("/transfers")
        .header("content-type", "application/json")
        .header("X-API-Key", api_key)
        .header("X-Request-User-Id", user_a_id.to_string())
        .body(Body::from(serde_json::to_string(&TransferRequest {
            from_user_id: user_a_id,
            to_user_id: user_b_id,
            amount: "120.00".to_string(),
            memo: Some("User history transfer".to_string()),
        }).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let account_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = $1 AND account_type = 'user_wallet' LIMIT 1",
    )
    .bind(user_a_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Out-of-range transaction should be filtered out
    let old_payload = serde_json::json!({
        "amount": "1.00000000",
        "description": "old_user_history_event"
    });
    sqlx::query(
        r#"
        INSERT INTO events (id, aggregate_type, aggregate_id, event_type, version, event_data, context, created_at)
        VALUES ($1, 'Account', $2, 'MoneyCredited', $3, $4, '{}', NOW() - INTERVAL '40 days')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(9901_i64)
    .bind(old_payload)
    .execute(&pool)
    .await
    .unwrap();

    // In-range but non transaction event should be filtered out
    let non_tx_payload = serde_json::json!({
        "description": "non_transaction_event"
    });
    sqlx::query(
        r#"
        INSERT INTO events (id, aggregate_type, aggregate_id, event_type, version, event_data, context, created_at)
        VALUES ($1, 'Account', $2, 'AccountCreated', $3, $4, '{}', NOW())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(9902_i64)
    .bind(non_tx_payload)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/users/{}/history", user_a_id))
        .header("X-API-Key", api_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["user_id"], user_a_id.to_string());
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "Expected at least one history entry");

    assert!(
        entries.iter().all(|e| {
            matches!(
                e["event_type"].as_str(),
                Some("MoneyCredited") | Some("MoneyDebited")
            )
        }),
        "Non-transaction events were included"
    );

    assert!(
        entries.iter().all(|e| e["description"] != "old_user_history_event"),
        "Out-of-range event was included"
    );
    assert!(
        entries.iter().all(|e| e["description"] != "non_transaction_event"),
        "Non transaction event was included"
    );

    let threshold = Utc::now() - Duration::days(31);
    let mut timestamps = Vec::new();
    for entry in entries {
        let created_at = entry["created_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap();
        assert!(
            created_at >= threshold,
            "Entry older than last month was included: {}",
            created_at
        );
        timestamps.push(created_at);
    }

    for pair in timestamps.windows(2) {
        assert!(
            pair[0] >= pair[1],
            "Entries are not sorted in descending created_at order"
        );
    }
}

#[tokio::test]
async fn test_user_history_requires_read_accounts_permission() {
    let pool = common::setup_test_db().await;
    let app = api::create_router()
        .layer(middleware::from_fn_with_state(pool.clone(), finance_atp::api::middleware::auth_middleware))
        .with_state(pool.clone());

    let limited_key = "limited_key_no_accounts_for_user_history";
    let limited_hash: String = sqlx::query_scalar(
        "SELECT encode(sha256($1::bytea), 'hex')",
    )
    .bind(limited_key.as_bytes())
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO api_keys (id, name, key_hash, key_prefix, permissions, is_active)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind("Limited Key User History")
    .bind(limited_hash)
    .bind("lim_")
    .bind(vec!["read:users".to_string()])
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/users/{}/history", Uuid::new_v4()))
        .header("X-API-Key", limited_key)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

//! API Integration Tests (M156-M158)

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    middleware,
};
use tower::util::ServiceExt;
use finance_atp::api::{self, routes::{CreateUserRequest, MintRequest, TransferRequest}};
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

//! Common test utilities

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Setup test database - truncate tables and seed test data
pub async fn setup_test_db() -> PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for tests");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to DB");

    // Compute hash dynamically to match what middleware expects
    let hash_check: String = sqlx::query_scalar("SELECT encode(sha256('test_key_123'::bytea), 'hex')")
        .fetch_one(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    // Clean up DB for fresh state
    sqlx::query("TRUNCATE TABLE events, event_snapshots, api_keys, accounts, users, idempotency_keys CASCADE")
        .execute(&mut *tx)
        .await
        .expect("Failed to clean up DB");

    // Seed test API Key with dynamically computed hash
    sqlx::query(
        r#"
        INSERT INTO api_keys (id, name, key_hash, key_prefix, permissions, is_active)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (key_prefix) DO NOTHING
        "#
    )
    .bind(uuid::Uuid::new_v4())
    .bind("Test Key")
    .bind(&hash_check)
    .bind("test_")
    .bind(vec!["admin".to_string(), "mint".to_string(), "read:users".to_string(), "write:users".to_string(), "write:transfers".to_string()])
    .bind(true)
    .execute(&mut *tx)
    .await
    .expect("Failed to seed API key");

    // Seed SYSTEM_MINT user and account (required for Mint operations)
    let system_user_id: uuid::Uuid = "00000000-0000-0000-0000-000000000001".parse().unwrap();
    let system_account_id: uuid::Uuid = "00000000-0000-0000-0000-000000000002".parse().unwrap();

    // 1. Insert System User
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, is_active, is_system, created_at, updated_at)
        VALUES ($1, 'system_mint', 'system@internal.test', true, true, NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#
    )
    .bind(system_user_id)
    .execute(&mut *tx)
    .await
    .expect("Failed to seed System User");

    // 2. Insert System Account
    sqlx::query(
        r#"
        INSERT INTO accounts (id, user_id, account_type, is_active, created_at)
        VALUES ($1, $2, 'mint_source', true, NOW())
        ON CONFLICT (id) DO NOTHING
        "#
    )
    .bind(system_account_id)
    .bind(system_user_id)
    .execute(&mut *tx)
    .await
    .expect("Failed to seed System Account");

    // 3. Insert AccountCreated Event (required for event sourcing)
    let event_id = uuid::Uuid::new_v4();
    let payload = serde_json::json!({
        "type": "AccountCreated",
        "account_id": system_account_id,
        "user_id": system_user_id,
        "account_type": "mint_source",
        "created_at": "2026-01-01T00:00:00Z"
    });

    sqlx::query(
        r#"
        INSERT INTO events (id, aggregate_type, aggregate_id, event_type, version, event_data, context, created_at)
        VALUES ($1, 'Account', $2, 'AccountCreated', 1, $3, '{}', '2026-01-01 00:00:00+00')
        "#
    )
    .bind(event_id)
    .bind(system_account_id)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .expect("Failed to seed System Account event");

    tx.commit().await.expect("Failed to commit transaction");

    pool
}

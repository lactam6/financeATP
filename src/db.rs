//! Database module
//!
//! Database connection and migration utilities.

use sqlx::PgPool;

/// Run database migrations
/// Note: We use raw SQL files in migrations/ directory
/// This function can be used to verify database connectivity
pub async fn verify_connection(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Simple connectivity check
    sqlx::query("SELECT 1")
        .execute(pool)
        .await?;
    
    Ok(())
}

/// Check if required tables exist
pub async fn check_schema(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let required_tables = vec![
        "api_keys",
        "rate_limit_buckets",
        "events",
        "event_snapshots",
        "users",
        "account_types",
        "accounts",
        "account_balances",
        "ledger_entries",
        "idempotency_keys",
        "audit_logs",
    ];

    for table in required_tables {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = $1
            )
            "#
        )
        .bind(table)
        .fetch_one(pool)
        .await?;

        if !exists {
            tracing::error!("Required table '{}' does not exist", table);
            return Ok(false);
        }
    }

    // Check for required system accounts
    if !check_system_accounts(pool).await? {
        return Ok(false);
    }

    Ok(true)
}

/// System user IDs
const SYSTEM_MINT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
const SYSTEM_BURN_USER_ID: &str = "00000000-0000-0000-0000-000000000002";

/// Check if required system accounts exist
async fn check_system_accounts(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let system_users = vec![
        (SYSTEM_MINT_USER_ID, "SYSTEM_MINT"),
        (SYSTEM_BURN_USER_ID, "SYSTEM_BURN"),
    ];

    for (user_id_str, name) in system_users {
        let user_id: uuid::Uuid = user_id_str.parse().expect("Invalid system user ID");
        
        // Check if user exists
        let user_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)"
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        if !user_exists {
            tracing::error!(
                "Required system user '{}' ({}) does not exist. Please run database seed.",
                name, user_id
            );
            return Ok(false);
        }

        // Check if account exists
        let account_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE user_id = $1)"
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        if !account_exists {
            tracing::error!(
                "Required system account for '{}' ({}) does not exist. Please run database seed.",
                name, user_id
            );
            return Ok(false);
        }
    }

    tracing::info!("System accounts verified: SYSTEM_MINT, SYSTEM_BURN");
    Ok(true)
}

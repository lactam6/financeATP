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

    Ok(true)
}

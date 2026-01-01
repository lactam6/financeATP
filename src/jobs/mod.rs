//! Scheduled Jobs
//!
//! Background jobs for periodic maintenance tasks.
//! These jobs are run on a schedule to clean up expired data and maintain system health.

use chrono::{DateTime, Datelike, Utc};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;

// =========================================================================
// M144: Rate Limit Bucket Cleanup Job
// =========================================================================

/// Clean up expired rate limit buckets
/// Removes buckets older than 2 minutes to prevent unbounded growth
pub async fn cleanup_rate_limit_buckets(pool: &PgPool) -> Result<u64, JobError> {
    let result = sqlx::query(
        r#"
        DELETE FROM rate_limit_buckets
        WHERE window_start < NOW() - INTERVAL '2 minutes'
        "#,
    )
    .execute(pool)
    .await?;

    let rows_deleted = result.rows_affected();
    
    if rows_deleted > 0 {
        tracing::info!(
            rows_deleted = rows_deleted,
            "Cleaned up expired rate limit buckets"
        );
    }

    Ok(rows_deleted)
}

// =========================================================================
// M145: Idempotency Key Timeout Reset Job
// =========================================================================

/// Reset stale idempotency keys that are stuck in 'processing' status
/// Keys stuck for more than 5 minutes are reset to 'failed' to allow retry
pub async fn reset_stale_idempotency_keys(pool: &PgPool) -> Result<u64, JobError> {
    let result = sqlx::query(
        r#"
        UPDATE idempotency_keys
        SET processing_status = 'failed'
        WHERE processing_status = 'processing'
          AND processing_started_at < NOW() - INTERVAL '5 minutes'
        "#,
    )
    .execute(pool)
    .await?;

    let rows_affected = result.rows_affected();
    
    if rows_affected > 0 {
        tracing::warn!(
            rows_affected = rows_affected,
            "Reset stale processing idempotency keys"
        );
    }

    Ok(rows_affected)
}

// =========================================================================
// M146: Expired Idempotency Key Deletion Job
// =========================================================================

/// Delete expired idempotency keys
/// Keys older than their expiration time (default 24 hours) are removed
pub async fn delete_expired_idempotency_keys(pool: &PgPool) -> Result<u64, JobError> {
    let result = sqlx::query(
        r#"
        DELETE FROM idempotency_keys
        WHERE expires_at < NOW()
        "#,
    )
    .execute(pool)
    .await?;

    let rows_deleted = result.rows_affected();
    
    if rows_deleted > 0 {
        tracing::info!(
            rows_deleted = rows_deleted,
            "Deleted expired idempotency keys"
        );
    }

    Ok(rows_deleted)
}

// =========================================================================
// M147: Monthly Partition Creation
// =========================================================================

/// Create partitions for the next month
/// Should be run near the end of each month to ensure partitions exist
pub async fn create_next_month_partitions(pool: &PgPool) -> Result<PartitionResult, JobError> {
    let now = Utc::now();
    let next_month = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    
    let month_after = if next_month.1 == 12 {
        (next_month.0 + 1, 1)
    } else {
        (next_month.0, next_month.1 + 1)
    };

    let partition_suffix = format!("{}_{:02}", next_month.0, next_month.1);
    let start_date = format!("{}-{:02}-01", next_month.0, next_month.1);
    let end_date = format!("{}-{:02}-01", month_after.0, month_after.1);

    let mut partitions_created = Vec::new();

    // Create events partition
    let events_partition = format!("events_{}", partition_suffix);
    if !partition_exists(pool, &events_partition).await? {
        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} PARTITION OF events
            FOR VALUES FROM ('{}') TO ('{}')
            "#,
            events_partition, start_date, end_date
        );
        sqlx::query(&sql).execute(pool).await?;
        partitions_created.push(events_partition.clone());
        tracing::info!(partition = %events_partition, "Created events partition");
    }

    // Create ledger_entries partition
    let ledger_partition = format!("ledger_entries_{}", partition_suffix);
    if !partition_exists(pool, &ledger_partition).await? {
        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} PARTITION OF ledger_entries
            FOR VALUES FROM ('{}') TO ('{}')
            "#,
            ledger_partition, start_date, end_date
        );
        sqlx::query(&sql).execute(pool).await?;
        partitions_created.push(ledger_partition.clone());
        tracing::info!(partition = %ledger_partition, "Created ledger_entries partition");
    }

    Ok(PartitionResult {
        partition_suffix,
        start_date,
        end_date,
        partitions_created,
    })
}

/// Check if a partition table already exists
async fn partition_exists(pool: &PgPool, table_name: &str) -> Result<bool, JobError> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_schema = 'public' AND table_name = $1
        )
        "#,
    )
    .bind(table_name)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Result of partition creation
#[derive(Debug, Clone)]
pub struct PartitionResult {
    pub partition_suffix: String,
    pub start_date: String,
    pub end_date: String,
    pub partitions_created: Vec<String>,
}

// =========================================================================
// Job Scheduler
// =========================================================================

/// Configuration for job scheduler
#[derive(Debug, Clone)]
pub struct JobSchedulerConfig {
    /// Interval for rate limit cleanup (default: 1 minute)
    pub rate_limit_cleanup_interval: Duration,
    /// Interval for idempotency key maintenance (default: 1 minute)
    pub idempotency_maintenance_interval: Duration,
    /// Interval for partition check (default: 1 hour)
    pub partition_check_interval: Duration,
}

impl Default for JobSchedulerConfig {
    fn default() -> Self {
        Self {
            rate_limit_cleanup_interval: Duration::from_secs(60),
            idempotency_maintenance_interval: Duration::from_secs(60),
            partition_check_interval: Duration::from_secs(3600),
        }
    }
}

/// Job Scheduler - runs periodic maintenance tasks
pub struct JobScheduler {
    pool: PgPool,
    config: JobSchedulerConfig,
}

impl JobScheduler {
    /// Create a new job scheduler
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            config: JobSchedulerConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(pool: PgPool, config: JobSchedulerConfig) -> Self {
        Self { pool, config }
    }

    /// Start the job scheduler in the background
    /// Returns a handle that can be used to abort the scheduler
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the scheduler loop
    async fn run(&self) {
        tracing::info!("Job scheduler started");

        let mut rate_limit_interval = interval(self.config.rate_limit_cleanup_interval);
        let mut idempotency_interval = interval(self.config.idempotency_maintenance_interval);
        let mut partition_interval = interval(self.config.partition_check_interval);

        loop {
            tokio::select! {
                _ = rate_limit_interval.tick() => {
                    if let Err(e) = cleanup_rate_limit_buckets(&self.pool).await {
                        tracing::error!(error = %e, "Rate limit cleanup failed");
                    }
                }
                _ = idempotency_interval.tick() => {
                    if let Err(e) = reset_stale_idempotency_keys(&self.pool).await {
                        tracing::error!(error = %e, "Idempotency key reset failed");
                    }
                    if let Err(e) = delete_expired_idempotency_keys(&self.pool).await {
                        tracing::error!(error = %e, "Idempotency key deletion failed");
                    }
                }
                _ = partition_interval.tick() => {
                    if should_create_partitions() {
                        if let Err(e) = create_next_month_partitions(&self.pool).await {
                            tracing::error!(error = %e, "Partition creation failed");
                        }
                    }
                }
            }
        }
    }

    /// Run all maintenance jobs once (for manual trigger or testing)
    pub async fn run_all_once(&self) -> MaintenanceReport {
        let mut report = MaintenanceReport::default();

        match cleanup_rate_limit_buckets(&self.pool).await {
            Ok(count) => report.rate_limit_buckets_cleaned = count,
            Err(e) => report.errors.push(format!("Rate limit cleanup: {}", e)),
        }

        match reset_stale_idempotency_keys(&self.pool).await {
            Ok(count) => report.idempotency_keys_reset = count,
            Err(e) => report.errors.push(format!("Idempotency reset: {}", e)),
        }

        match delete_expired_idempotency_keys(&self.pool).await {
            Ok(count) => report.idempotency_keys_deleted = count,
            Err(e) => report.errors.push(format!("Idempotency deletion: {}", e)),
        }

        if should_create_partitions() {
            match create_next_month_partitions(&self.pool).await {
                Ok(result) => report.partitions_created = result.partitions_created,
                Err(e) => report.errors.push(format!("Partition creation: {}", e)),
            }
        }

        report.completed_at = Utc::now();
        report
    }
}

/// Check if we should create partitions (last 3 days of month)
fn should_create_partitions() -> bool {
    let now = Utc::now();
    let days_in_month = days_in_month(now.year(), now.month());
    now.day() >= days_in_month - 3
}

/// Get the number of days in a month
fn days_in_month(year: i32, month: u32) -> u32 {
    if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .signed_duration_since(chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap())
    .num_days() as u32
}

/// Report from running maintenance jobs
#[derive(Debug, Clone, Default)]
pub struct MaintenanceReport {
    pub rate_limit_buckets_cleaned: u64,
    pub idempotency_keys_reset: u64,
    pub idempotency_keys_deleted: u64,
    pub partitions_created: Vec<String>,
    pub errors: Vec<String>,
    pub completed_at: DateTime<Utc>,
}

/// Job execution errors
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_in_month() {
        // January 2026
        assert_eq!(days_in_month(2026, 1), 31);
        // February 2026 (not leap year)
        assert_eq!(days_in_month(2026, 2), 28);
        // February 2024 (leap year)
        assert_eq!(days_in_month(2024, 2), 29);
        // April
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn test_job_scheduler_config_default() {
        let config = JobSchedulerConfig::default();
        assert_eq!(config.rate_limit_cleanup_interval, Duration::from_secs(60));
        assert_eq!(config.idempotency_maintenance_interval, Duration::from_secs(60));
        assert_eq!(config.partition_check_interval, Duration::from_secs(3600));
    }

    #[test]
    fn test_partition_result() {
        let result = PartitionResult {
            partition_suffix: "2026_02".to_string(),
            start_date: "2026-02-01".to_string(),
            end_date: "2026-03-01".to_string(),
            partitions_created: vec!["events_2026_02".to_string()],
        };

        assert_eq!(result.partitions_created.len(), 1);
    }

    #[test]
    fn test_maintenance_report_default() {
        let report = MaintenanceReport::default();
        assert_eq!(report.rate_limit_buckets_cleaned, 0);
        assert_eq!(report.errors.len(), 0);
    }
}

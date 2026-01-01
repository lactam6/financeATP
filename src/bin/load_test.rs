//! Load Testing Tool (M160, M161)
//!
//! Run with: cargo run --bin load_test --release -- --events 1000

use std::time::Instant;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let event_count: u64 = args.iter()
        .position(|a| a == "--events")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let database_url = std::env::var("DATABASE_URL")?;
    
    println!("Load Test - Inserting {} events", event_count);
    println!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    let start = Instant::now();
    let mut success_count = 0u64;

    for i in 0..event_count {
        let event_id = uuid::Uuid::new_v4();
        let aggregate_id = uuid::Uuid::new_v4();
        let payload = serde_json::json!({
            "type": "AccountCreated",
            "account_id": aggregate_id,
            "user_id": uuid::Uuid::new_v4(),
            "account_type": "user_wallet",
            "created_at": chrono::Utc::now()
        });

        let result = sqlx::query(
            r#"
            INSERT INTO events (id, aggregate_type, aggregate_id, event_type, version, event_data, created_at)
            VALUES ($1, 'Account', $2, 'AccountCreated', 1, $3, NOW())
            "#
        )
        .bind(event_id)
        .bind(aggregate_id)
        .bind(&payload)
        .execute(&pool)
        .await;

        if result.is_ok() {
            success_count += 1;
        }

        if (i + 1) % 1000 == 0 {
            println!("Inserted {} events...", i + 1);
        }
    }

    let elapsed = start.elapsed();
    let rate = success_count as f64 / elapsed.as_secs_f64();

    println!("\n=== Load Test Results ===");
    println!("Total events: {}", event_count);
    println!("Successful: {}", success_count);
    println!("Time: {:.2}s", elapsed.as_secs_f64());
    println!("Rate: {:.0} events/sec", rate);

    Ok(())
}

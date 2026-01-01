//! financeATP - ATP Currency Management Backend API
//! 
//! This is an internal backend API for managing the ATP currency.
//! It uses Event Sourcing and Double-Entry Bookkeeping for robust financial transactions.

use std::net::SocketAddr;

use axum::{middleware, Router};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod aggregate;
pub mod api;
pub mod audit;
mod config;
mod db;
pub mod domain;
mod error;
pub mod event_store;
pub mod handlers;
pub mod idempotency;
pub mod jobs;
pub mod projection;

pub use config::Config;
pub use error::{AppError, AppResult};

/// Initialize tracing/logging
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "finance_atp=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Build the application router
fn build_router(pool: PgPool) -> Router {
    // Create API router with all routes
    let api_router = api::create_router();

    // Apply middleware to API routes
    // Note: Axum layers are applied in reverse order (last added = first executed)
    // Order: logging -> auth -> rate_limit -> handler
    let protected_routes = api_router
        .layer(middleware::from_fn_with_state(
            pool.clone(),
            api::middleware::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            pool.clone(),
            api::middleware::auth_middleware,
        ))
        .layer(middleware::from_fn(
            api::middleware::logging_middleware,
        ));

    Router::new()
        // Health check (no auth)
        .route("/health", axum::routing::get(health_check))
        // Protected API routes
        .nest("/api/v1", protected_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(pool)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    init_tracing();

    // Load configuration
    let config = Config::from_env()?;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    tracing::info!("Starting financeATP server");
    tracing::info!("Connecting to database...");

    // Create database pool
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await?;

    // Verify database schema
    if !db::check_schema(&pool).await? {
        tracing::error!("Database schema is not complete. Please run migrations.");
        return Err(anyhow::anyhow!("Database schema incomplete"));
    }

    tracing::info!("Database connected successfully");
    tracing::info!("Listening on http://{}", addr);

    // Build router and start server
    let app = build_router(pool.clone());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    // M140: Graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Cleanup
    tracing::info!("Server shutting down...");
    pool.close().await;
    tracing::info!("Database connections closed. Goodbye!");

    Ok(())
}

/// M140: Shutdown signal handler for graceful shutdown
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, initiating graceful shutdown...");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown...");
        },
    }
}
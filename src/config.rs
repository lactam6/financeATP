//! Configuration module
//!
//! Loads configuration from environment variables.

use std::env;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Database connection URL
    pub database_url: String,
    
    /// Maximum database connections in pool
    pub database_max_connections: u32,
    
    /// Server host
    pub host: String,
    
    /// Server port
    pub port: u16,
    
    /// Environment (development, production)
    pub environment: String,
    
    /// Rate limit: requests per minute per API key
    pub rate_limit_per_minute: i32,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnv("DATABASE_URL"))?;

        let database_max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidValue("DATABASE_MAX_CONNECTIONS"))?;

        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidValue("PORT"))?;

        let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

        let rate_limit_per_minute = env::var("RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidValue("RATE_LIMIT_PER_MINUTE"))?;

        Ok(Self {
            database_url,
            database_max_connections,
            host,
            port,
            environment,
            rate_limit_per_minute,
        })
    }

    /// Check if running in production
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }
}

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing environment variable: {0}")]
    MissingEnv(&'static str),

    #[error("Invalid value for environment variable: {0}")]
    InvalidValue(&'static str),
}

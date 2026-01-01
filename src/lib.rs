//! financeATP Library
//!
//! Re-exports modules for integration testing and external use.

pub mod aggregate;
pub mod api;
pub mod audit;
pub mod domain;
pub mod event_store;
pub mod handlers;
pub mod idempotency;
pub mod jobs;
pub mod projection;

// Private modules (used only by main.rs binary)
pub mod config;
pub mod db;
mod error;

pub use config::Config;
pub use error::{AppError, AppResult};
pub use domain::{Amount, AmountError, Balance, OperationContext, DomainError};
pub use domain::{AccountEvent, TransferEvent, UserEvent};

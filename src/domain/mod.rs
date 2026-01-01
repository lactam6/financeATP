//! Domain module
//!
//! Core domain types and business logic.

pub mod amount;
pub mod context;
pub mod error;
pub mod events;

pub use amount::{Amount, AmountError, Balance};
pub use context::OperationContext;
pub use error::DomainError;
pub use events::{AccountEvent, TransferEvent, UserEvent, UserChanges, TransferFailureReason};

//! Idempotency module
//!
//! Prevents duplicate request processing using idempotency keys.

mod repository;

pub use repository::{IdempotencyRepository, IdempotencyKey, IdempotencyStatus};

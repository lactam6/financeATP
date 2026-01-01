//! Event Store module
//!
//! Persistence layer for Event Sourcing.
//! Handles storing and retrieving events from PostgreSQL.

mod error;
mod repository;

pub use error::EventStoreError;
pub use repository::{EventStore, AggregateOperation, StoredEvent};

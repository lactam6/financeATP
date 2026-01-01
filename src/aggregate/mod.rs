//! Aggregate module
//!
//! Aggregate Root pattern implementation for Event Sourcing.

pub mod account;
pub mod user;

pub use account::Account;
pub use user::User;

/// Aggregate trait that all aggregates must implement
pub trait Aggregate: Sized + Default {
    /// The type of events this aggregate handles
    type Event;

    /// Get the aggregate type name (for storage)
    fn aggregate_type() -> &'static str;

    /// Get the aggregate ID
    fn id(&self) -> uuid::Uuid;

    /// Get the current version (number of events applied)
    fn version(&self) -> i64;

    /// Apply an event to update the aggregate state
    fn apply(self, event: Self::Event) -> Self;

    /// Check if a snapshot should be created
    fn should_snapshot(&self) -> bool {
        const SNAPSHOT_INTERVAL: i64 = 100;
        self.version() > 0 && self.version() % SNAPSHOT_INTERVAL == 0
    }
}

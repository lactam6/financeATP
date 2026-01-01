//! Projection module
//!
//! Updates read-model tables (projections) from events.
//! Projections are optimized for queries and derived from events.

mod service;

pub use service::ProjectionService;

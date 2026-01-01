//! API module
//!
//! HTTP API endpoints and middleware.

pub mod middleware;
pub mod routes;

pub use routes::create_router;

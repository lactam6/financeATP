//! Command Handlers module
//!
//! CQRS Command handlers that orchestrate business operations.
//! Each handler coordinates aggregates, event store, and projections.

mod commands;
mod user_handler;
mod transfer_handler;
mod mint_handler;
mod burn_handler;
mod update_user_handler;
mod deactivate_user_handler;

#[cfg(test)]
mod tests;

pub use commands::*;
pub use user_handler::CreateUserHandler;
pub use transfer_handler::TransferHandler;
pub use mint_handler::MintHandler;
pub use burn_handler::{BurnHandler, BurnCommand, BurnResult};
pub use update_user_handler::{UpdateUserHandler, UpdateUserCommand, UpdateUserResult};
pub use deactivate_user_handler::{DeactivateUserHandler, DeactivateUserCommand, DeactivateUserResult};


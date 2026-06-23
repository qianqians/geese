//! Unified physics backend manager.
//!
//! Provides [`PhysicsManager`] that can run physics locally (in-process
//! via `physics` crate), remotely (via `physics_client` + TCP), or both.
//! Independent of editor, server, and client specifics.

mod manager;
pub use manager::{PhysicsManager, PhysicsSource};

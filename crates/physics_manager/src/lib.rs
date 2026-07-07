//! Local physics manager.
//!
//! Provides [`PhysicsManager`] that runs physics locally via the `physics` crate.

mod manager;
pub use manager::{BodySnapshot, PhysicsManager, Position3, Quat4};

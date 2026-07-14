//! Shader composition framework for the Geese engine.
//!
//! Provides a type-safe, composable shader system built on top of WGSL/wgpu.
//! Inspired by Stride's SDSL+SDFX approach.

pub mod core;
pub mod stream;
pub mod generator;
pub mod compose;
pub mod effect;
pub mod variant;

pub use self::core::*;
pub use stream::*;

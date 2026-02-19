//! Bose Search Engine — shared types.
//!
//! Common data structures, error types, and configuration shared across all crates.

#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod blending;
pub mod chunker;
pub mod config;
pub mod dedup;
pub mod error;
pub mod extractor;
pub mod fusion;
pub mod pid;
pub mod scavenger;
pub mod types;

pub use blending::*;
pub use chunker::*;
pub use config::*;
pub use dedup::*;
pub use error::*;
pub use extractor::*;
pub use fusion::*;
pub use pid::*;
pub use scavenger::{ScavengerConfig, ScavengerResult, scavenge};
pub use types::*;

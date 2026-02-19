//! Bose MCP Server — Model Context Protocol interface.
//!
//! Exposes bose-search tools via the MCP protocol using rmcp.

#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod router;

pub use router::{QueryExpansionRouter, SkipDecision};

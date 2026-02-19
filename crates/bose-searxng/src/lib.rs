//! `SearXNG` client — meta-search engine HTTP interface.

#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod client;
pub mod response;

pub use client::SearxngClient;

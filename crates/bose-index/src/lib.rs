//! Bose Search Engine — local index engine.
//!
//! Local knowledge index built on Tantivy.

pub mod indexer;
pub mod schema;
pub mod searcher;

pub use indexer::{Document, Indexer};
pub use schema::create_schema;
pub use searcher::{SearchResult, Searcher};

//! Bose Search Engine — 本地索引引擎
//!
//! 基於 Tantivy 的本地知識庫索引。

pub mod indexer;
pub mod schema;
pub mod searcher;

pub use indexer::{Document, Indexer};
pub use searcher::{SearchResult, Searcher};
pub use schema::create_schema;

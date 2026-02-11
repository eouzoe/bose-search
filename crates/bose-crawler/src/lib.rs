//! Bose Search Engine — 爬蟲模組
//!
//! 基於 tokio + reqwest 的並發 HTTP 爬蟲。
//! 未來可透過 `io-uring` feature 切換到 monoio。

pub mod fetcher;
pub mod metrics;

pub use fetcher::{FetchError, FetchResult, Fetcher};
pub use metrics::CrawlerMetrics;

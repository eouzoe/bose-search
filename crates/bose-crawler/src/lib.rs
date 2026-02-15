//! Bose Search Engine — crawler module.
//!
//! Concurrent HTTP crawler built on tokio + reqwest.
//! Can switch to monoio via the `io-uring` feature in the future.

pub mod fetcher;
pub mod metrics;
pub mod monitor;

pub use fetcher::{FetchError, FetchResult, Fetcher};
pub use metrics::CrawlerMetrics;
pub use monitor::{CpuMonitor, SystemMonitor, SystemSnapshot};

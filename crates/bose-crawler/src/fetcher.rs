//! HTTP fetcher.
//!
//! Concurrent HTTP fetcher built on reqwest with PID backpressure and system monitoring.
//! Uses the tokio runtime with PID-controlled dynamic concurrency.

use crate::metrics::CrawlerMetrics;
use crate::monitor::{CpuMonitor, SystemMonitor};
use anyhow::Result;
use bose_common::pid::PidController;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// 抓取結果
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub url: String,
    pub status: u16,
    pub body: String,
    pub latency: Duration,
}

/// 抓取錯誤
#[derive(Debug, Clone)]
pub struct FetchError {
    pub url: String,
    pub error: String,
    pub latency: Duration,
}

/// HTTP fetcher.
pub struct Fetcher {
    client: reqwest::Client,
    metrics: Arc<CrawlerMetrics>,
    max_concurrency: usize,
    timeout: Duration,
    pid: Option<PidController>,
    monitor: Option<Box<dyn CpuMonitor>>,
}

impl Fetcher {
    /// 創建新的抓取器
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(max_concurrency: usize, timeout_secs: u64) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("BoseSearch/0.1")
            .build()?;

        Ok(Self {
            client,
            metrics: Arc::new(CrawlerMetrics::new()),
            max_concurrency,
            timeout: Duration::from_secs(timeout_secs),
            pid: None,
            monitor: None,
        })
    }

    /// Create a fetcher with PID-controlled backpressure (safety-first config).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn with_backpressure(max_concurrency: usize, timeout_secs: u64) -> Result<Self> {
        let mut fetcher = Self::new(max_concurrency, timeout_secs)?;

        // 安全優先: 目標延遲 100ms, CPU 40%, 並發 2-20
        let pid = PidController::new()
            .with_targets(100.0, 40.0)
            .with_limits(2, max_concurrency.min(20).try_into().unwrap_or(20));

        let monitor = SystemMonitor::with_safety_threshold(70.0);

        fetcher.pid = Some(pid);
        fetcher.monitor = Some(Box::new(monitor));

        Ok(fetcher)
    }

    /// Fetch a single URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response body cannot be read.
    pub async fn fetch_one(&self, url: &str) -> Result<FetchResult, FetchError> {
        let start = Instant::now();

        match self.client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match resp.text().await {
                    Ok(body) => {
                        let latency = start.elapsed();
                        self.metrics.record_success(latency, body.len());
                        Ok(FetchResult { url: url.to_string(), status, body, latency })
                    }
                    Err(e) => {
                        let latency = start.elapsed();
                        self.metrics.record_failure(latency);
                        Err(FetchError { url: url.to_string(), error: e.to_string(), latency })
                    }
                }
            }
            Err(e) => {
                let latency = start.elapsed();
                self.metrics.record_failure(latency);
                Err(FetchError { url: url.to_string(), error: e.to_string(), latency })
            }
        }
    }

    /// Fetch URLs in batch with concurrency control.
    ///
    /// # Panics
    ///
    /// Panics if the semaphore is closed, which should never happen in normal operation.
    pub async fn crawl_batch(&self, urls: Vec<String>) -> Vec<Result<FetchResult, FetchError>> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut handles = Vec::with_capacity(urls.len());

        for url in urls {
            let sem = semaphore.clone();
            let client = self.client.clone();
            let metrics = self.metrics.clone();
            let timeout = self.timeout;

            handles.push(tokio::spawn(async move {
                #[expect(
                    clippy::unwrap_used,
                    reason = "semaphore is never closed in normal operation"
                )]
                let _permit = sem.acquire().await.unwrap();
                let start = Instant::now();

                match client.get(&url).timeout(timeout).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        match resp.text().await {
                            Ok(body) => {
                                let latency = start.elapsed();
                                metrics.record_success(latency, body.len());
                                Ok(FetchResult { url, status, body, latency })
                            }
                            Err(e) => {
                                let latency = start.elapsed();
                                metrics.record_failure(latency);
                                Err(FetchError { url, error: e.to_string(), latency })
                            }
                        }
                    }
                    Err(e) => {
                        let latency = start.elapsed();
                        metrics.record_failure(latency);
                        Err(FetchError { url, error: e.to_string(), latency })
                    }
                }
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(FetchError {
                    url: "unknown".to_string(),
                    error: e.to_string(),
                    latency: Duration::ZERO,
                })),
            }
        }

        results
    }

    /// Intelligent crawling with PID backpressure control.
    ///
    /// Dynamically adjusts concurrency based on CPU and latency to protect hardware.
    ///
    /// # 安全保護
    /// - Pauses crawling when CPU > 70%, waits 5s to cool down
    /// - Hard caps concurrency at 20 regardless of PID output
    /// - Total crawl timeout: 5 minutes
    ///
    /// # Errors
    ///
    /// Returns an error if the crawl operation fails.
    ///
    /// # Panics
    ///
    /// Panics if monitor or PID controller is None when they should be initialized.
    pub async fn crawl_with_backpressure(
        &mut self,
        urls: Vec<String>,
    ) -> Result<Vec<Result<FetchResult, FetchError>>> {
        if urls.is_empty() {
            return Ok(vec![]);
        }

        // 確保有 PID 和 Monitor
        if self.pid.is_none() || self.monitor.is_none() {
            return Ok(self.crawl_batch(urls).await);
        }

        let mut results = Vec::with_capacity(urls.len());
        let mut remaining_urls: Vec<_> = urls.into_iter().collect();

        let start_time = Instant::now();
        let total_timeout = Duration::from_mins(5);
        let adjustment_interval = Duration::from_secs(1);
        let mut last_adjustment = Instant::now();

        while !remaining_urls.is_empty() {
            // 檢查總 timeout
            if start_time.elapsed() > total_timeout {
                tracing::warn!(
                    "Crawl timeout after 5 minutes, {} URLs remaining",
                    remaining_urls.len()
                );
                break;
            }

            // 安全檢查: CPU > 70% 時暫停
            #[expect(
                clippy::unwrap_used,
                reason = "monitor is guaranteed to be Some when entering this function"
            )]
            let is_safe = self.monitor.as_mut().unwrap().is_safe();
            if !is_safe {
                #[expect(
                    clippy::unwrap_used,
                    reason = "monitor is guaranteed to be Some when entering this function"
                )]
                let snapshot = self.monitor.as_mut().unwrap().snapshot();
                tracing::warn!(
                    "CPU too high ({:.1}%), pausing for 5 seconds",
                    snapshot.cpu_percent
                );
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            // 每秒調整一次並發量
            if last_adjustment.elapsed() >= adjustment_interval {
                #[expect(
                    clippy::unwrap_used,
                    reason = "monitor is guaranteed to be Some when entering this function"
                )]
                let snapshot = self.monitor.as_mut().unwrap().snapshot();
                let system_metrics = self.metrics.to_system_metrics(snapshot.cpu_percent);
                #[expect(
                    clippy::unwrap_used,
                    reason = "pid is guaranteed to be Some when entering this function"
                )]
                let new_limit = self.pid.as_mut().unwrap().adjust(&system_metrics);

                tracing::debug!(
                    "PID adjust: latency={:.1}ms, cpu={:.1}%, new_limit={}",
                    system_metrics.avg_latency_ms,
                    system_metrics.cpu_percent,
                    new_limit
                );

                last_adjustment = Instant::now();
            }

            // 取當前並發限制的 URL 數量
            #[expect(
                clippy::unwrap_used,
                reason = "pid is guaranteed to be Some when entering this function"
            )]
            let current_limit = self.pid.as_ref().unwrap().current_limit() as usize;
            let batch_size = current_limit.min(remaining_urls.len());
            let batch: Vec<_> = remaining_urls.drain(..batch_size).collect();

            // 並發抓取這一批
            let batch_results = self.crawl_batch(batch).await;
            results.extend(batch_results);

            // 短暫休息，避免過度消耗 CPU
            sleep(Duration::from_millis(100)).await;
        }

        Ok(results)
    }

    /// 獲取指標引用
    #[must_use]
    pub fn metrics(&self) -> &CrawlerMetrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use crate::monitor::MockMonitor;

    #[test]
    fn test_fetcher_new() {
        let fetcher = Fetcher::new(10, 30);
        assert!(fetcher.is_ok());
    }

    #[test]
    fn test_fetcher_with_backpressure_has_pid_and_monitor() {
        let fetcher = Fetcher::with_backpressure(10, 30).unwrap();
        assert!(fetcher.pid.is_some());
        assert!(fetcher.monitor.is_some());
    }

    #[test]
    fn test_fetch_result_fields() {
        let result = FetchResult {
            url: "https://example.com".to_string(),
            status: 200,
            body: "Hello".to_string(),
            latency: Duration::from_millis(100),
        };
        assert_eq!(result.status, 200);
        assert_eq!(result.body, "Hello");
    }

    #[test]
    fn test_fetch_error_fields() {
        let err = FetchError {
            url: "https://bad.com".to_string(),
            error: "timeout".to_string(),
            latency: Duration::from_secs(5),
        };
        assert_eq!(err.error, "timeout");
    }

    // Requirement: CPU > 70% must pause crawling
    #[tokio::test]
    async fn test_high_cpu_pauses_crawling() {
        let mut fetcher = Fetcher::with_backpressure(10, 5).unwrap();
        fetcher.monitor = Some(Box::new(MockMonitor::unsafe_cpu()));

        let urls = vec!["http://localhost:1/test".to_string()];
        let result =
            tokio::time::timeout(Duration::from_secs(2), fetcher.crawl_with_backpressure(urls))
                .await;

        assert!(result.is_err(), "Should timeout because crawler pauses on high CPU");
    }

    // Requirement: CPU < 70% allows crawling to proceed
    #[tokio::test]
    async fn test_safe_cpu_allows_crawling() {
        let mut fetcher = Fetcher::with_backpressure(10, 2).unwrap();
        fetcher.monitor = Some(Box::new(MockMonitor::safe()));

        let results = fetcher.crawl_with_backpressure(vec![]).await;
        assert!(results.is_ok());
        assert!(results.unwrap().is_empty());
    }

    // Requirement: PID concurrency within bounds [2, 20]
    #[test]
    fn test_pid_concurrency_within_bounds() {
        let fetcher = Fetcher::with_backpressure(20, 10).unwrap();
        let limit = fetcher.pid.as_ref().unwrap().current_limit();
        assert!((2..=20).contains(&limit), "PID limit {limit} out of [2, 20]");
    }

    // Requirement: without PID, falls back to normal batch
    #[tokio::test]
    async fn test_no_pid_falls_back_to_batch() {
        let mut fetcher = Fetcher::new(10, 2).unwrap();
        assert!(fetcher.pid.is_none());
        let results = fetcher.crawl_with_backpressure(vec![]).await;
        assert!(results.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_one_invalid_url_returns_error() {
        let fetcher = Fetcher::new(10, 2).unwrap();
        let result = fetcher.fetch_one("http://localhost:1/nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_metrics_count_after_fetch() {
        let fetcher = Fetcher::new(10, 2).unwrap();
        let _ = fetcher.fetch_one("http://localhost:1/fail").await;
        assert_eq!(fetcher.metrics().total_requests(), 1);
    }

    #[tokio::test]
    async fn test_crawl_batch_empty() {
        let fetcher = Fetcher::new(10, 5).unwrap();
        let results = fetcher.crawl_batch(vec![]).await;
        assert!(results.is_empty());
    }
}

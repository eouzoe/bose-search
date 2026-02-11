//! HTTP 抓取器
//!
//! 基於 tokio + reqwest 的並發 HTTP 抓取器。
//! 未來可透過 `io-uring` feature 切換到 monoio。

use crate::metrics::CrawlerMetrics;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

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

/// HTTP 抓取器
pub struct Fetcher {
    client: reqwest::Client,
    metrics: Arc<CrawlerMetrics>,
    max_concurrency: usize,
    timeout: Duration,
}

impl Fetcher {
    /// 創建新的抓取器
    pub fn new(
        max_concurrency: usize,
        timeout_secs: u64,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("BoseSearch/0.1")
            .build()?;

        Ok(Self {
            client,
            metrics: Arc::new(CrawlerMetrics::new()),
            max_concurrency,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    /// 抓取單個 URL
    pub async fn fetch_one(&self, url: &str) -> Result<FetchResult, FetchError> {
        let start = Instant::now();

        match self.client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match resp.text().await {
                    Ok(body) => {
                        let latency = start.elapsed();
                        self.metrics.record_success(latency, body.len());
                        Ok(FetchResult {
                            url: url.to_string(),
                            status,
                            body,
                            latency,
                        })
                    }
                    Err(e) => {
                        let latency = start.elapsed();
                        self.metrics.record_failure(latency);
                        Err(FetchError {
                            url: url.to_string(),
                            error: e.to_string(),
                            latency,
                        })
                    }
                }
            }
            Err(e) => {
                let latency = start.elapsed();
                self.metrics.record_failure(latency);
                Err(FetchError {
                    url: url.to_string(),
                    error: e.to_string(),
                    latency,
                })
            }
        }
    }

    /// 批量抓取 URL (並發控制)
    pub async fn crawl_batch(&self, urls: Vec<String>) -> Vec<Result<FetchResult, FetchError>> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut handles = Vec::with_capacity(urls.len());

        for url in urls {
            let sem = semaphore.clone();
            let client = self.client.clone();
            let metrics = self.metrics.clone();
            let timeout = self.timeout;

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let start = Instant::now();

                match client.get(&url).timeout(timeout).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        match resp.text().await {
                            Ok(body) => {
                                let latency = start.elapsed();
                                metrics.record_success(latency, body.len());
                                Ok(FetchResult {
                                    url,
                                    status,
                                    body,
                                    latency,
                                })
                            }
                            Err(e) => {
                                let latency = start.elapsed();
                                metrics.record_failure(latency);
                                Err(FetchError {
                                    url,
                                    error: e.to_string(),
                                    latency,
                                })
                            }
                        }
                    }
                    Err(e) => {
                        let latency = start.elapsed();
                        metrics.record_failure(latency);
                        Err(FetchError {
                            url,
                            error: e.to_string(),
                            latency,
                        })
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

    /// 獲取指標引用
    pub fn metrics(&self) -> &CrawlerMetrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_new() {
        let fetcher = Fetcher::new(10, 30);
        assert!(fetcher.is_ok());
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
            latency: Duration::from_millis(5000),
        };
        assert_eq!(err.error, "timeout");
    }

    #[tokio::test]
    async fn test_fetch_one_invalid_url() {
        let fetcher = Fetcher::new(10, 5).unwrap();
        let result = fetcher.fetch_one("http://localhost:1/nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_crawl_batch_empty() {
        let fetcher = Fetcher::new(10, 5).unwrap();
        let results = fetcher.crawl_batch(vec![]).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_after_fetch() {
        let fetcher = Fetcher::new(10, 2).unwrap();
        let _ = fetcher.fetch_one("http://localhost:1/fail").await;
        assert_eq!(fetcher.metrics().total_requests(), 1);
    }
}

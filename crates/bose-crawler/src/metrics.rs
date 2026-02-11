//! 爬蟲指標收集
//!
//! 追蹤爬蟲的延遲、成功率、錯誤等指標。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 爬蟲指標
#[derive(Debug, Default)]
pub struct CrawlerMetrics {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    total_latency_us: AtomicU64,
    total_bytes: AtomicU64,
}

impl CrawlerMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 記錄成功的請求
    pub fn record_success(&self, latency: Duration, bytes: usize) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
        self.total_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// 記錄失敗的請求
    pub fn record_failure(&self, latency: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    /// 獲取平均延遲 (毫秒)
    pub fn avg_latency_ms(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let total_us = self.total_latency_us.load(Ordering::Relaxed);
        (total_us as f64 / total as f64) / 1000.0
    }

    /// 獲取總請求數
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// 獲取成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 1.0;
        }
        let success = self.successful_requests.load(Ordering::Relaxed);
        success as f64 / total as f64
    }

    /// 獲取總下載位元組數
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// 轉換為 PID 控制器所需的 SystemMetrics
    pub fn to_system_metrics(&self, cpu_percent: f64) -> bose_common::pid::SystemMetrics {
        bose_common::pid::SystemMetrics {
            avg_latency_ms: self.avg_latency_ms(),
            cpu_percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let m = CrawlerMetrics::new();
        assert_eq!(m.total_requests(), 0);
        assert_eq!(m.avg_latency_ms(), 0.0);
        assert_eq!(m.success_rate(), 1.0);
    }

    #[test]
    fn test_record_success() {
        let m = CrawlerMetrics::new();
        m.record_success(Duration::from_millis(100), 1024);
        assert_eq!(m.total_requests(), 1);
        assert!((m.avg_latency_ms() - 100.0).abs() < 1.0);
        assert_eq!(m.total_bytes(), 1024);
        assert_eq!(m.success_rate(), 1.0);
    }

    #[test]
    fn test_record_failure() {
        let m = CrawlerMetrics::new();
        m.record_failure(Duration::from_millis(50));
        assert_eq!(m.total_requests(), 1);
        assert_eq!(m.success_rate(), 0.0);
    }

    #[test]
    fn test_mixed_results() {
        let m = CrawlerMetrics::new();
        m.record_success(Duration::from_millis(100), 500);
        m.record_success(Duration::from_millis(200), 500);
        m.record_failure(Duration::from_millis(300));

        assert_eq!(m.total_requests(), 3);
        assert!((m.avg_latency_ms() - 200.0).abs() < 1.0);
        assert!((m.success_rate() - 0.666).abs() < 0.01);
        assert_eq!(m.total_bytes(), 1000);
    }

    #[test]
    fn test_to_system_metrics() {
        let m = CrawlerMetrics::new();
        m.record_success(Duration::from_millis(100), 0);
        let sm = m.to_system_metrics(50.0);
        assert!((sm.avg_latency_ms - 100.0).abs() < 1.0);
        assert_eq!(sm.cpu_percent, 50.0);
    }
}

//! System monitoring module.
//!
//! CPU and memory monitoring to protect hardware from overload.

use sysinfo::System;

/// CPU monitor trait — injectable for testing.
pub trait CpuMonitor: Send {
    fn is_safe(&mut self) -> bool;
    fn snapshot(&mut self) -> SystemSnapshot;
}

/// System monitor.
///
/// Monitors CPU usage and memory, providing safety threshold protection.
pub struct SystemMonitor {
    sys: System,
    safety_threshold_cpu: f64,
}

impl SystemMonitor {
    /// Create a new system monitor with default CPU safety threshold of 70%.
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            safety_threshold_cpu: 70.0,
        }
    }

    /// Create a system monitor with a custom CPU safety threshold.
    pub fn with_safety_threshold(cpu_percent: f64) -> Self {
        Self {
            sys: System::new_all(),
            safety_threshold_cpu: cpu_percent,
        }
    }
}

impl CpuMonitor for SystemMonitor {
    /// Check whether the system is safe (CPU below threshold).
    fn is_safe(&mut self) -> bool {
        self.sys.refresh_cpu_usage();
        let cpu_percent = self.sys.global_cpu_usage() as f64;
        cpu_percent < self.safety_threshold_cpu
    }

    /// Take a snapshot of current system metrics.
    ///
    /// Refreshes CPU usage before reading.
    fn snapshot(&mut self) -> SystemSnapshot {
        self.sys.refresh_cpu_usage();

        let cpu_percent = self.sys.global_cpu_usage() as f64;
        let memory_used_mb = self.sys.used_memory() / 1024 / 1024;
        let memory_total_mb = self.sys.total_memory() / 1024 / 1024;

        SystemSnapshot {
            cpu_percent,
            memory_used_mb,
            memory_total_mb,
        }
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// System snapshot.
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    /// CPU usage percentage (0–100)
    pub cpu_percent: f64,
    /// Used memory (MB)
    pub memory_used_mb: u64,
    /// Total memory (MB)
    pub memory_total_mb: u64,
}

#[cfg(test)]
pub struct MockMonitor {
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub safety_threshold: f64,
}

#[cfg(test)]
impl MockMonitor {
    pub fn safe() -> Self {
        Self {
            cpu_percent: 20.0,
            memory_used_mb: 2048,
            memory_total_mb: 8192,
            safety_threshold: 70.0,
        }
    }

    pub fn unsafe_cpu() -> Self {
        Self {
            cpu_percent: 85.0,
            memory_used_mb: 2048,
            memory_total_mb: 8192,
            safety_threshold: 70.0,
        }
    }

    pub fn with_cpu(cpu_percent: f64, threshold: f64) -> Self {
        Self {
            cpu_percent,
            memory_used_mb: 2048,
            memory_total_mb: 8192,
            safety_threshold: threshold,
        }
    }
}

#[cfg(test)]
impl CpuMonitor for MockMonitor {
    fn is_safe(&mut self) -> bool {
        self.cpu_percent < self.safety_threshold
    }

    fn snapshot(&mut self) -> SystemSnapshot {
        SystemSnapshot {
            cpu_percent: self.cpu_percent,
            memory_used_mb: self.memory_used_mb,
            memory_total_mb: self.memory_total_mb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_new() {
        let monitor = SystemMonitor::new();
        assert_eq!(monitor.safety_threshold_cpu, 70.0);
    }

    #[test]
    fn test_snapshot_returns_valid_data() {
        let mut monitor = SystemMonitor::new();
        let snapshot = monitor.snapshot();
        assert!(snapshot.cpu_percent >= 0.0 && snapshot.cpu_percent <= 100.0);
        assert!(snapshot.memory_total_mb > 0);
        assert!(snapshot.memory_used_mb <= snapshot.memory_total_mb);
    }

    #[test]
    fn test_mock_safe_cpu_is_safe() {
        let mut mock = MockMonitor::safe();
        assert!(mock.is_safe(), "CPU 20% should be safe (threshold 70%)");
    }

    #[test]
    fn test_mock_unsafe_cpu_is_not_safe() {
        let mut mock = MockMonitor::unsafe_cpu();
        assert!(
            !mock.is_safe(),
            "CPU 85% should NOT be safe (threshold 70%)"
        );
    }

    #[test]
    fn test_mock_custom_threshold() {
        let mut mock = MockMonitor::with_cpu(60.0, 50.0);
        assert!(
            !mock.is_safe(),
            "CPU 60% should NOT be safe (threshold 50%)"
        );

        let mut mock2 = MockMonitor::with_cpu(40.0, 50.0);
        assert!(mock2.is_safe(), "CPU 40% should be safe (threshold 50%)");
    }

    #[test]
    fn test_mock_snapshot_returns_injected_values() {
        let mut mock = MockMonitor::with_cpu(55.0, 70.0);
        let snap = mock.snapshot();
        assert!((snap.cpu_percent - 55.0).abs() < f64::EPSILON);
        assert_eq!(snap.memory_used_mb, 2048);
        assert_eq!(snap.memory_total_mb, 8192);
    }
}

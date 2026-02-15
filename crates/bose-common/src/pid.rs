//! PID backpressure controller.
//!
//! Dynamically adjusts concurrency based on system metrics (latency, CPU).
//! Uses the classic PID control algorithm, called once per second.
//!
//! # Design
//!
//! - Target latency: 100ms
//! - Target CPU: 60%
//! - Concurrency range: [5, 200]
//! - Multi-signal fusion: max(latency_error, cpu_error * 2)
//!
//! # Examples
//!
//! ```
//! use bose_common::pid::{PidController, SystemMetrics};
//!
//! let mut pid = PidController::new();
//! let metrics = SystemMetrics {
//!     avg_latency_ms: 80.0,
//!     cpu_percent: 40.0,
//! };
//! let concurrency = pid.adjust(&metrics);
//! assert!(concurrency >= 5);
//! assert!(concurrency <= 200);
//! ```

/// 系統指標
#[derive(Debug, Clone, Copy)]
pub struct SystemMetrics {
    /// 平均延遲 (毫秒)
    pub avg_latency_ms: f64,
    /// CPU usage (percentage)
    pub cpu_percent: f64,
}

/// PID controller.
///
/// 使用比例-積分-微分控制來動態調整並發量。
pub struct PidController {
    // PID 增益
    kp: f64,
    ki: f64,
    kd: f64,

    // 目標值
    target_latency_ms: f64,
    target_cpu_percent: f64,

    // 並發限制
    min_concurrency: u32,
    max_concurrency: u32,

    // PID 狀態
    integral: f64,
    prev_error: f64,
    current_limit: f64,
}

impl PidController {
    /// Create a new PID controller with default parameters.
    pub fn new() -> Self {
        Self {
            kp: 0.5,
            ki: 0.1,
            kd: 0.05,
            target_latency_ms: 100.0,
            target_cpu_percent: 60.0,
            min_concurrency: 5,
            max_concurrency: 200,
            integral: 0.0,
            prev_error: 0.0,
            current_limit: 100.0, // 初始並發量
        }
    }

    /// Customise PID gains.
    pub fn with_gains(mut self, kp: f64, ki: f64, kd: f64) -> Self {
        self.kp = kp;
        self.ki = ki;
        self.kd = kd;
        self
    }

    /// 自訂目標值
    pub fn with_targets(mut self, latency_ms: f64, cpu_percent: f64) -> Self {
        self.target_latency_ms = latency_ms;
        self.target_cpu_percent = cpu_percent;
        self
    }

    /// 自訂並發範圍
    pub fn with_limits(mut self, min: u32, max: u32) -> Self {
        self.min_concurrency = min;
        self.max_concurrency = max;
        self.current_limit = (min as f64 + max as f64) / 2.0;
        self
    }

    /// 根據系統指標調整並發量
    ///
    /// 每秒調用一次，返回新的並發限制。
    pub fn adjust(&mut self, metrics: &SystemMetrics) -> u32 {
        // 計算誤差 (正值 = 超過目標，需要降低並發)
        let latency_error = metrics.avg_latency_ms - self.target_latency_ms;
        let cpu_error = (metrics.cpu_percent - self.target_cpu_percent) * 2.0;

        // 多信號融合: 取最大誤差
        let error = latency_error.max(cpu_error);

        // PID 計算
        self.integral += error;
        // 防止積分飽和
        let max_integral = self.max_concurrency as f64;
        self.integral = self.integral.clamp(-max_integral, max_integral);

        let derivative = error - self.prev_error;
        self.prev_error = error;

        let output = self.kp * error + self.ki * self.integral + self.kd * derivative;

        // 調整並發量 (誤差正 → 降低並發)
        self.current_limit -= output;
        self.current_limit = self
            .current_limit
            .clamp(self.min_concurrency as f64, self.max_concurrency as f64);

        self.current_limit as u32
    }

    /// 獲取當前並發限制
    pub fn current_limit(&self) -> u32 {
        self.current_limit as u32
    }

    /// Reset PID state.
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
        self.current_limit = (self.min_concurrency as f64 + self.max_concurrency as f64) / 2.0;
    }
}

impl Default for PidController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_new() {
        let pid = PidController::new();
        assert_eq!(pid.current_limit(), 100);
    }

    #[test]
    fn test_pid_default() {
        let pid = PidController::default();
        assert_eq!(pid.current_limit(), 100);
    }

    #[test]
    fn test_adjust_low_load() {
        let mut pid = PidController::new();
        // 低負載: 延遲低、CPU 低 → 應該增加並發
        let metrics = SystemMetrics {
            avg_latency_ms: 50.0,
            cpu_percent: 30.0,
        };
        let limit = pid.adjust(&metrics);
        assert!(
            limit > 100,
            "Should increase concurrency under low load, got {limit}"
        );
    }

    #[test]
    fn test_adjust_high_load() {
        let mut pid = PidController::new();
        // 高負載: 延遲高、CPU 高 → 應該降低並發
        let metrics = SystemMetrics {
            avg_latency_ms: 200.0,
            cpu_percent: 90.0,
        };
        let limit = pid.adjust(&metrics);
        assert!(
            limit < 100,
            "Should decrease concurrency under high load, got {limit}"
        );
    }

    #[test]
    fn test_adjust_respects_min() {
        let mut pid = PidController::new();
        // 極高負載，多次調整
        let metrics = SystemMetrics {
            avg_latency_ms: 1000.0,
            cpu_percent: 100.0,
        };
        for _ in 0..100 {
            pid.adjust(&metrics);
        }
        assert!(
            pid.current_limit() >= 5,
            "Should not go below min_concurrency"
        );
    }

    #[test]
    fn test_adjust_respects_max() {
        let mut pid = PidController::new();
        // 極低負載，多次調整
        let metrics = SystemMetrics {
            avg_latency_ms: 1.0,
            cpu_percent: 1.0,
        };
        for _ in 0..100 {
            pid.adjust(&metrics);
        }
        assert!(
            pid.current_limit() <= 200,
            "Should not exceed max_concurrency"
        );
    }

    #[test]
    fn test_convergence() {
        let mut pid = PidController::new();
        // 模擬穩定在目標附近
        let metrics = SystemMetrics {
            avg_latency_ms: 100.0,
            cpu_percent: 60.0,
        };
        let initial = pid.current_limit();
        for _ in 0..10 {
            pid.adjust(&metrics);
        }
        // 在目標值時，並發量應該穩定
        let final_limit = pid.current_limit();
        let diff = (final_limit as i32 - initial as i32).unsigned_abs();
        assert!(diff < 10, "Should be stable near target, diff={diff}");
    }

    #[test]
    fn test_custom_gains() {
        let pid = PidController::new().with_gains(1.0, 0.2, 0.1);
        assert_eq!(pid.kp, 1.0);
        assert_eq!(pid.ki, 0.2);
        assert_eq!(pid.kd, 0.1);
    }

    #[test]
    fn test_custom_limits() {
        let pid = PidController::new().with_limits(10, 500);
        assert_eq!(pid.min_concurrency, 10);
        assert_eq!(pid.max_concurrency, 500);
        assert_eq!(pid.current_limit(), 255); // (10+500)/2
    }

    #[test]
    fn test_reset() {
        let mut pid = PidController::new();
        let metrics = SystemMetrics {
            avg_latency_ms: 200.0,
            cpu_percent: 90.0,
        };
        pid.adjust(&metrics);
        pid.adjust(&metrics);

        pid.reset();
        assert_eq!(pid.current_limit(), 102); // (5+200)/2
        assert_eq!(pid.integral, 0.0);
        assert_eq!(pid.prev_error, 0.0);
    }

    #[test]
    fn test_cpu_dominates_when_high() {
        let mut pid = PidController::new();
        // CPU 很高但延遲正常 → CPU 誤差應該主導
        let metrics = SystemMetrics {
            avg_latency_ms: 100.0, // 正好在目標
            cpu_percent: 95.0,     // 遠超目標
        };
        let limit = pid.adjust(&metrics);
        assert!(
            limit < 100,
            "CPU overload should reduce concurrency, got {limit}"
        );
    }
}

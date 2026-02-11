use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Trait for types that have a score
///
/// 實作此 trait 的類型可以用於 `QueryExpansionRouter::should_skip_expansion()`
pub trait HasScore {
    /// 返回結果的分數（0.0 到 1.0）
    fn score(&self) -> f64;
}

/// 查詢擴展路由器配置
///
/// 根據搜尋結果的置信度決定是否跳過查詢擴展。
/// 當 top score 足夠高且與第二名差距夠大時，跳過擴展以降低延遲。
///
/// # Examples
///
/// ```
/// use bose_mcp::router::QueryExpansionRouter;
///
/// let router = QueryExpansionRouter::default();
/// // 使用 router.should_skip_expansion() 決定是否跳過擴展
/// ```
#[derive(Debug, Clone)]
pub struct QueryExpansionRouter {
    /// Top score 閾值（預設 0.85）
    skip_threshold: f64,
    /// Score gap 閾值（預設 0.15）
    gap_threshold: f64,
}

impl Default for QueryExpansionRouter {
    fn default() -> Self {
        Self {
            skip_threshold: 0.85,
            gap_threshold: 0.15,
        }
    }
}

impl QueryExpansionRouter {
    /// 從環境變數建立路由器
    ///
    /// 支援的環境變數：
    /// - `QUERY_EXPANSION_SKIP_THRESHOLD`: Top score 閾值（預設 0.85）
    /// - `QUERY_EXPANSION_GAP_THRESHOLD`: Gap 閾值（預設 0.15）
    pub fn from_env() -> Self {
        let skip_threshold = std::env::var("QUERY_EXPANSION_SKIP_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.85);

        let gap_threshold = std::env::var("QUERY_EXPANSION_GAP_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.15);

        Self {
            skip_threshold,
            gap_threshold,
        }
    }

    /// 決定是否跳過查詢擴展
    ///
    /// 根據搜尋結果的置信度決定是否需要查詢擴展：
    /// - 如果 top_score ≥ skip_threshold 且 gap ≥ gap_threshold，跳過擴展
    /// - 否則，執行查詢擴展
    ///
    /// # Arguments
    ///
    /// * `results` - 搜尋結果列表，必須實作 `HasScore` trait
    ///
    /// # Returns
    ///
    /// 返回 `SkipDecision`，包含決策結果和原因
    ///
    /// # Examples
    ///
    /// ```
    /// use bose_mcp::router::QueryExpansionRouter;
    ///
    /// #[derive(Clone)]
    /// struct SearchResult { score: f64 }
    ///
    /// impl bose_mcp::router::HasScore for SearchResult {
    ///     fn score(&self) -> f64 { self.score }
    /// }
    ///
    /// let router = QueryExpansionRouter::default();
    /// let results = vec![
    ///     SearchResult { score: 0.92 },
    ///     SearchResult { score: 0.65 },
    /// ];
    ///
    /// let decision = router.should_skip_expansion(&results);
    /// assert!(decision.skip);
    /// ```
    pub fn should_skip_expansion<T: HasScore>(&self, results: &[T]) -> SkipDecision {
        if results.is_empty() {
            debug!("No results, expansion may help");
            return SkipDecision {
                skip: false,
                reason: "No results, expansion may help".to_string(),
                top_score: None,
                gap: None,
            };
        }

        let top_score = results[0].score();

        // 計算 gap
        let gap = if results.len() > 1 {
            top_score - results[1].score()
        } else {
            1.0 // 只有一個結果，gap 設為最大
        };

        let skip = top_score >= self.skip_threshold && gap >= self.gap_threshold;

        let reason = if skip {
            format!(
                "High confidence: top_score={:.2}, gap={:.2}",
                top_score, gap
            )
        } else {
            format!(
                "Low confidence: top_score={:.2} (threshold={:.2}), gap={:.2} (threshold={:.2})",
                top_score, self.skip_threshold, gap, self.gap_threshold
            )
        };

        if skip {
            info!("Skipping query expansion: {}", reason);
        } else {
            debug!("Executing query expansion: {}", reason);
        }

        SkipDecision {
            skip,
            reason,
            top_score: Some(top_score),
            gap: Some(gap),
        }
    }
}

/// 跳過決策結果
///
/// 包含決策結果、原因以及相關的分數資訊。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkipDecision {
    /// 是否跳過擴展
    pub skip: bool,
    /// 決策原因
    pub reason: String,
    /// Top score（如果有結果）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_score: Option<f64>,
    /// Score gap（如果有結果）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 模擬搜尋結果
    #[derive(Debug, Clone)]
    struct MockSearchResult {
        score: f64,
    }

    impl HasScore for MockSearchResult {
        fn score(&self) -> f64 {
            self.score
        }
    }

    #[test]
    fn test_default_thresholds() {
        let router = QueryExpansionRouter::default();
        assert_eq!(router.skip_threshold, 0.85);
        assert_eq!(router.gap_threshold, 0.15);
    }

    #[test]
    fn test_from_env_defaults() {
        // 清除環境變數
        unsafe {
            std::env::remove_var("QUERY_EXPANSION_SKIP_THRESHOLD");
            std::env::remove_var("QUERY_EXPANSION_GAP_THRESHOLD");
        }

        let router = QueryExpansionRouter::from_env();
        assert_eq!(router.skip_threshold, 0.85);
        assert_eq!(router.gap_threshold, 0.15);
    }

    #[test]
    fn test_from_env_custom() {
        unsafe {
            std::env::set_var("QUERY_EXPANSION_SKIP_THRESHOLD", "0.9");
            std::env::set_var("QUERY_EXPANSION_GAP_THRESHOLD", "0.2");
        }

        let router = QueryExpansionRouter::from_env();
        assert_eq!(router.skip_threshold, 0.9);
        assert_eq!(router.gap_threshold, 0.2);

        // 清理
        unsafe {
            std::env::remove_var("QUERY_EXPANSION_SKIP_THRESHOLD");
            std::env::remove_var("QUERY_EXPANSION_GAP_THRESHOLD");
        }
    }

    // Scenario 1: 高置信度結果（跳過擴展）
    #[test]
    fn test_skip_high_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            MockSearchResult { score: 0.92 },
            MockSearchResult { score: 0.65 },
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip expansion for high confidence");
        assert_eq!(decision.top_score, Some(0.92));
        assert_eq!(decision.gap, Some(0.27));
        assert!(decision.reason.contains("High confidence"));
    }

    // Scenario 2: 低置信度結果（執行擴展）
    #[test]
    fn test_expand_low_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            MockSearchResult { score: 0.78 },
            MockSearchResult { score: 0.72 },
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(!decision.skip, "Should expand for low confidence");
        assert_eq!(decision.top_score, Some(0.78));
        // 使用近似比較
        assert!((decision.gap.unwrap() - 0.06).abs() < 0.001);
        assert!(decision.reason.contains("Low confidence"));
    }

    // Scenario 3: 邊界條件（top score 剛好 0.85）
    #[test]
    fn test_boundary_skip() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            MockSearchResult { score: 0.85 },
            MockSearchResult { score: 0.68 },
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip when score equals threshold");
        assert_eq!(decision.top_score, Some(0.85));
        // 使用近似比較
        assert!((decision.gap.unwrap() - 0.17).abs() < 0.001);
    }

    // Scenario 4: 只有一個結果
    #[test]
    fn test_single_result() {
        let router = QueryExpansionRouter::default();
        let results = vec![MockSearchResult { score: 0.90 }];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip for single high-score result");
        assert_eq!(decision.top_score, Some(0.90));
        assert_eq!(decision.gap, Some(1.0));
    }

    // Scenario 5: 空結果
    #[test]
    fn test_empty_results() {
        let router = QueryExpansionRouter::default();
        let results: Vec<MockSearchResult> = vec![];

        let decision = router.should_skip_expansion(&results);
        assert!(!decision.skip, "Should expand for empty results");
        assert_eq!(decision.top_score, None);
        assert_eq!(decision.gap, None);
        assert!(decision.reason.contains("No results"));
    }

    // 邊界測試：gap 剛好等於閾值
    #[test]
    fn test_gap_boundary() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            MockSearchResult { score: 0.90 },
            MockSearchResult { score: 0.75 }, // gap = 0.15
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip when gap equals threshold");
    }

    // 邊界測試：gap 略小於閾值
    #[test]
    fn test_gap_below_threshold() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            MockSearchResult { score: 0.90 },
            MockSearchResult { score: 0.751 }, // gap = 0.149
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(!decision.skip, "Should expand when gap below threshold");
    }
}

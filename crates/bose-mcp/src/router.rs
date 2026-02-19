use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Trait for types that carry a relevance score.
///
/// Implementors can be fed into [`QueryExpansionRouter::should_skip_expansion`].
pub trait HasScore {
    /// Returns the relevance score in the range `0.0..=1.0`.
    fn score(&self) -> f64;
}

/// Decides whether query expansion is worth the extra latency.
///
/// When the top result is already highly confident (score ≥ threshold)
/// and clearly separated from the runner-up (gap ≥ threshold), expansion
/// is skipped to save time.
///
/// # Examples
///
/// ```
/// use bose_search::router::QueryExpansionRouter;
///
/// let router = QueryExpansionRouter::default();
/// // Use router.should_skip_expansion() to decide
/// ```
#[derive(Debug, Clone)]
pub struct QueryExpansionRouter {
    /// Minimum top score to consider skipping (default: 0.85)
    skip_threshold: f64,
    /// Minimum gap between #1 and #2 (default: 0.15)
    gap_threshold: f64,
}

impl Default for QueryExpansionRouter {
    fn default() -> Self {
        Self { skip_threshold: 0.85, gap_threshold: 0.15 }
    }
}

impl QueryExpansionRouter {
    /// Build from environment variables.
    ///
    /// Reads:
    /// - `QUERY_EXPANSION_SKIP_THRESHOLD` (default: 0.85)
    /// - `QUERY_EXPANSION_GAP_THRESHOLD` (default: 0.15)
    #[must_use]
    pub fn from_env() -> Self {
        let skip_threshold = std::env::var("QUERY_EXPANSION_SKIP_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.85);

        let gap_threshold = std::env::var("QUERY_EXPANSION_GAP_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.15);

        Self { skip_threshold, gap_threshold }
    }

    /// Decide whether to skip query expansion.
    ///
    /// Returns [`SkipDecision::skip`] = `true` when the top result is
    /// confident enough on its own (score ≥ threshold, gap ≥ threshold).
    ///
    /// # Arguments
    ///
    /// * `results` — ranked search results implementing [`HasScore`]
    ///
    /// # Returns
    ///
    /// A [`SkipDecision`] with the verdict and supporting metrics.
    ///
    /// # Examples
    ///
    /// ```
    /// use bose_search::router::QueryExpansionRouter;
    ///
    /// #[derive(Clone)]
    /// struct SearchResult { score: f64 }
    ///
    /// impl bose_search::router::HasScore for SearchResult {
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

        // Compute gap
        let gap = if results.len() > 1 {
            top_score - results[1].score()
        } else {
            1.0 // Single result — treat gap as maximum
        };

        let skip = top_score >= self.skip_threshold && gap >= self.gap_threshold;

        let reason = if skip {
            format!("High confidence: top_score={top_score:.2}, gap={gap:.2}")
        } else {
            format!(
                "Low confidence: top_score={top_score:.2} (threshold={:.2}), gap={gap:.2} (threshold={:.2})",
                self.skip_threshold, self.gap_threshold
            )
        };

        if skip {
            info!("Skipping query expansion: {}", reason);
        } else {
            debug!("Executing query expansion: {}", reason);
        }

        SkipDecision { skip, reason, top_score: Some(top_score), gap: Some(gap) }
    }
}

/// The outcome of a skip-expansion check.
///
/// Carries the boolean verdict, a human-readable reason, and the
/// metrics that informed the decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkipDecision {
    /// Whether expansion should be skipped
    pub skip: bool,
    /// Human-readable explanation
    pub reason: String,
    /// Top result score, if any results were present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_score: Option<f64>,
    /// Gap between #1 and #2, if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
}

#[cfg(test)]
mod tests {
    #![expect(clippy::float_cmp, reason = "test: comparing exact literal values")]
    #![expect(clippy::unwrap_used, reason = "test code")]

    use super::*;

    // Mock search result for testing
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
        // Clear env vars
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

        // Cleanup
        unsafe {
            std::env::remove_var("QUERY_EXPANSION_SKIP_THRESHOLD");
            std::env::remove_var("QUERY_EXPANSION_GAP_THRESHOLD");
        }
    }

    // Scenario 1: high-confidence result — skip expansion
    #[test]
    fn test_skip_high_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![MockSearchResult { score: 0.92 }, MockSearchResult { score: 0.65 }];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip expansion for high confidence");
        assert_eq!(decision.top_score, Some(0.92));
        assert_eq!(decision.gap, Some(0.27));
        assert!(decision.reason.contains("High confidence"));
    }

    // Scenario 2: low-confidence result — expand
    #[test]
    fn test_expand_low_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![MockSearchResult { score: 0.78 }, MockSearchResult { score: 0.72 }];

        let decision = router.should_skip_expansion(&results);
        assert!(!decision.skip, "Should expand for low confidence");
        assert_eq!(decision.top_score, Some(0.78));
        // Approximate comparison
        assert!((decision.gap.unwrap() - 0.06).abs() < 0.001);
        assert!(decision.reason.contains("Low confidence"));
    }

    // Scenario 3: boundary — top score exactly at threshold
    #[test]
    fn test_boundary_skip() {
        let router = QueryExpansionRouter::default();
        let results = vec![MockSearchResult { score: 0.85 }, MockSearchResult { score: 0.68 }];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip when score equals threshold");
        assert_eq!(decision.top_score, Some(0.85));
        // Approximate comparison
        assert!((decision.gap.unwrap() - 0.17).abs() < 0.001);
    }

    // Scenario 4: single result
    #[test]
    fn test_single_result() {
        let router = QueryExpansionRouter::default();
        let results = vec![MockSearchResult { score: 0.90 }];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip, "Should skip for single high-score result");
        assert_eq!(decision.top_score, Some(0.90));
        assert_eq!(decision.gap, Some(1.0));
    }

    // Scenario 5: empty results
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

    // Boundary: gap exactly at threshold
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

    // Boundary: gap just below threshold
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

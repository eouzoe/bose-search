use std::collections::HashMap;

/// Reciprocal Rank Fusion (RRF) algorithm.
///
/// Merges results from multiple search engines. Rank-based, robust to outliers.
///
/// # Algorithm
///
/// ```text
/// RRF_score = weight / (k + rank + 1)
/// ```
///
/// Where:
/// - k = 60 (constant, Cormack et al. 2009)
/// - rank is the 0-based position in that engine
/// - weight is the engine weight
///
/// # Examples
///
/// ```
/// use bose_common::fusion::{RRFFusion, RankedResult};
///
/// let mut fusion = RRFFusion::new();
///
/// // 添加第一個引擎的結果
/// fusion.add_results("engine_a", vec![
///     RankedResult { id: "doc1".to_string(), rank: 0 },
///     RankedResult { id: "doc2".to_string(), rank: 1 },
/// ], 1.0);
///
/// // 添加第二個引擎的結果
/// fusion.add_results("engine_b", vec![
///     RankedResult { id: "doc2".to_string(), rank: 0 },
///     RankedResult { id: "doc3".to_string(), rank: 1 },
/// ], 1.0);
///
/// // 融合結果
/// let fused = fusion.fuse(10);
/// assert_eq!(fused[0].id, "doc2"); // doc2 ranks high in both engines
/// ```
#[derive(Debug, Clone)]
pub struct RRFFusion {
    /// RRF constant k (default: 60)
    k: f64,
    /// Accumulated document scores
    scores: HashMap<String, f64>,
    /// Best rank per document (for top-rank bonus)
    best_ranks: HashMap<String, usize>,
}

impl Default for RRFFusion {
    fn default() -> Self {
        Self::new()
    }
}

impl RRFFusion {
    /// Create a new RRF fusion instance.
    #[must_use]
    pub fn new() -> Self {
        Self { k: 60.0, scores: HashMap::new(), best_ranks: HashMap::new() }
    }

    /// Create a fusion instance with a custom k value.
    #[must_use]
    pub fn with_k(k: f64) -> Self {
        Self { k, scores: HashMap::new(), best_ranks: HashMap::new() }
    }

    /// Add results from one engine.
    ///
    /// # Arguments
    ///
    /// * `source` — engine name (for logging)
    /// * `results` — ranked result list
    /// * `weight` — engine weight (use 2.0 for the original query)
    pub fn add_results(&mut self, _source: &str, results: Vec<RankedResult>, weight: f64) {
        for result in results {
            #[expect(
                clippy::cast_precision_loss,
                reason = "rank is small, precision loss negligible"
            )]
            let rrf_score = weight / (self.k + result.rank as f64 + 1.0);

            // Accumulate score
            *self.scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;

            // Track best rank
            self.best_ranks
                .entry(result.id.clone())
                .and_modify(|best| *best = (*best).min(result.rank))
                .or_insert(result.rank);
        }
    }

    /// Fuse and return results sorted by RRF score.
    ///
    /// # Arguments
    ///
    /// * `top_n` — return the top N results
    ///
    /// # Returns
    ///
    /// Results sorted by descending RRF score.
    #[must_use]
    pub fn fuse(&self, top_n: usize) -> Vec<FusedResult> {
        let mut results: Vec<FusedResult> = self
            .scores
            .iter()
            .map(|(id, &score)| {
                let best_rank = self.best_ranks.get(id).copied().unwrap_or(usize::MAX);

                // Top-rank bonus
                let bonus = match best_rank {
                    0 => 0.05,
                    1 => 0.03,
                    _ => 0.0,
                };

                FusedResult { id: id.clone(), score: score + bonus, best_rank }
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        results.into_iter().take(top_n).collect()
    }
}

/// A ranked result (input to fusion).
#[derive(Debug, Clone)]
pub struct RankedResult {
    /// Document ID
    pub id: String,
    /// Rank (0-based)
    pub rank: usize,
}

/// A fused result (output of fusion).
#[derive(Debug, Clone, PartialEq)]
pub struct FusedResult {
    /// Document ID
    pub id: String,
    /// RRF score
    pub score: f64,
    /// Best rank across engines
    pub best_rank: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_engine() {
        let mut fusion = RRFFusion::new();
        fusion.add_results(
            "engine_a",
            vec![
                RankedResult { id: "doc1".to_string(), rank: 0 },
                RankedResult { id: "doc2".to_string(), rank: 1 },
                RankedResult { id: "doc3".to_string(), rank: 2 },
            ],
            1.0,
        );

        let fused = fusion.fuse(10);
        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
        assert_eq!(fused[2].id, "doc3");
    }

    #[test]
    fn test_two_engines_agreement() {
        let mut fusion = RRFFusion::new();

        // Both engines rank doc2 highly
        fusion.add_results(
            "engine_a",
            vec![
                RankedResult { id: "doc1".to_string(), rank: 0 },
                RankedResult { id: "doc2".to_string(), rank: 1 },
            ],
            1.0,
        );

        fusion.add_results(
            "engine_b",
            vec![
                RankedResult { id: "doc2".to_string(), rank: 0 },
                RankedResult { id: "doc3".to_string(), rank: 1 },
            ],
            1.0,
        );

        let fused = fusion.fuse(10);
        // doc2 should rank first (high in both engines)
        assert_eq!(fused[0].id, "doc2");
    }

    #[test]
    fn test_original_query_weight() {
        let mut fusion = RRFFusion::new();

        // Original query weight ×2
        fusion.add_results("original", vec![RankedResult { id: "doc1".to_string(), rank: 0 }], 2.0);

        // Expanded query weight ×1
        fusion.add_results("expanded", vec![RankedResult { id: "doc2".to_string(), rank: 0 }], 1.0);

        let fused = fusion.fuse(10);
        // doc1 should rank first (higher weight)
        assert_eq!(fused[0].id, "doc1");
        assert!(fused[0].score > fused[1].score);
    }

    #[test]
    fn test_top_rank_bonus() {
        let mut fusion = RRFFusion::new();

        fusion.add_results(
            "engine",
            vec![
                RankedResult { id: "doc1".to_string(), rank: 0 }, // Gets +0.05 bonus
                RankedResult { id: "doc2".to_string(), rank: 1 }, // Gets +0.03 bonus
                RankedResult { id: "doc3".to_string(), rank: 2 }, // No bonus
            ],
            1.0,
        );

        let fused = fusion.fuse(10);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
        assert_eq!(fused[2].id, "doc3");

        // Verify bonus was applied
        assert!(fused[0].score > 1.0 / 61.0 + 0.04); // Base score + bonus
    }

    #[test]
    fn test_empty_results() {
        let fusion = RRFFusion::new();
        let fused = fusion.fuse(10);
        assert_eq!(fused.len(), 0);
    }

    #[test]
    fn test_top_n_limit() {
        let mut fusion = RRFFusion::new();

        fusion.add_results(
            "engine",
            vec![
                RankedResult { id: "doc1".to_string(), rank: 0 },
                RankedResult { id: "doc2".to_string(), rank: 1 },
                RankedResult { id: "doc3".to_string(), rank: 2 },
            ],
            1.0,
        );

        let fused = fusion.fuse(2);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
    }

    #[test]
    fn test_custom_k() {
        let mut fusion = RRFFusion::with_k(30.0);

        fusion.add_results("engine", vec![RankedResult { id: "doc1".to_string(), rank: 0 }], 1.0);

        let fused = fusion.fuse(1);
        // With k=30: score = 1/(30+0+1) + 0.05 bonus

        assert!((fused[0].score - (1.0 / 31.0 + 0.05)).abs() < 0.001);
    }
}

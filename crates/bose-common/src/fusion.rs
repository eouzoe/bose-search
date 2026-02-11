use std::collections::HashMap;

/// Reciprocal Rank Fusion (RRF) 演算法
///
/// 用於合併多個搜尋引擎的結果。基於排名而非分數，對異常值魯棒。
///
/// # Algorithm
///
/// ```text
/// RRF_score = weight / (k + rank + 1)
/// ```
///
/// 其中：
/// - k = 60 (常數，來自 Cormack et al. 2009)
/// - rank 是結果在該引擎中的排名（0-based）
/// - weight 是引擎權重
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
/// assert_eq!(fused[0].id, "doc2"); // doc2 在兩個引擎都排名靠前
/// ```
#[derive(Debug, Clone)]
pub struct RRFFusion {
    /// RRF 常數 k（預設 60）
    k: f64,
    /// 累積的文檔分數
    scores: HashMap<String, f64>,
    /// 每個文檔的最佳排名（用於 top-rank 加成）
    best_ranks: HashMap<String, usize>,
}

impl Default for RRFFusion {
    fn default() -> Self {
        Self::new()
    }
}

impl RRFFusion {
    /// 建立新的 RRF 融合器
    pub fn new() -> Self {
        Self {
            k: 60.0,
            scores: HashMap::new(),
            best_ranks: HashMap::new(),
        }
    }

    /// 建立自訂 k 值的融合器
    pub fn with_k(k: f64) -> Self {
        Self {
            k,
            scores: HashMap::new(),
            best_ranks: HashMap::new(),
        }
    }

    /// 添加一個引擎的結果
    ///
    /// # Arguments
    ///
    /// * `source` - 引擎名稱（用於日誌）
    /// * `results` - 排名結果列表
    /// * `weight` - 引擎權重（原始查詢建議用 2.0）
    pub fn add_results(&mut self, _source: &str, results: Vec<RankedResult>, weight: f64) {
        for result in results {
            let rrf_score = weight / (self.k + result.rank as f64 + 1.0);

            // 累積分數
            *self.scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;

            // 更新最佳排名
            self.best_ranks
                .entry(result.id.clone())
                .and_modify(|best| *best = (*best).min(result.rank))
                .or_insert(result.rank);
        }
    }

    /// 融合結果並返回排序後的列表
    ///
    /// # Arguments
    ///
    /// * `top_n` - 返回前 N 個結果
    ///
    /// # Returns
    ///
    /// 返回按 RRF 分數排序的結果列表
    pub fn fuse(&self, top_n: usize) -> Vec<FusedResult> {
        let mut results: Vec<FusedResult> = self
            .scores
            .iter()
            .map(|(id, &score)| {
                let best_rank = self.best_ranks.get(id).copied().unwrap_or(usize::MAX);

                // Top-rank 加成
                let bonus = match best_rank {
                    0 => 0.05,
                    1 => 0.03,
                    _ => 0.0,
                };

                FusedResult {
                    id: id.clone(),
                    score: score + bonus,
                    best_rank,
                }
            })
            .collect();

        // 按分數降序排序
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.into_iter().take(top_n).collect()
    }
}

/// 排名結果（輸入）
#[derive(Debug, Clone)]
pub struct RankedResult {
    /// 文檔 ID
    pub id: String,
    /// 排名（0-based）
    pub rank: usize,
}

/// 融合結果（輸出）
#[derive(Debug, Clone, PartialEq)]
pub struct FusedResult {
    /// 文檔 ID
    pub id: String,
    /// RRF 分數
    pub score: f64,
    /// 最佳排名
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
                RankedResult {
                    id: "doc1".to_string(),
                    rank: 0,
                },
                RankedResult {
                    id: "doc2".to_string(),
                    rank: 1,
                },
                RankedResult {
                    id: "doc3".to_string(),
                    rank: 2,
                },
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

        // 兩個引擎都認為 doc2 最好
        fusion.add_results(
            "engine_a",
            vec![
                RankedResult {
                    id: "doc1".to_string(),
                    rank: 0,
                },
                RankedResult {
                    id: "doc2".to_string(),
                    rank: 1,
                },
            ],
            1.0,
        );

        fusion.add_results(
            "engine_b",
            vec![
                RankedResult {
                    id: "doc2".to_string(),
                    rank: 0,
                },
                RankedResult {
                    id: "doc3".to_string(),
                    rank: 1,
                },
            ],
            1.0,
        );

        let fused = fusion.fuse(10);
        // doc2 應該排第一（在兩個引擎都有好排名）
        assert_eq!(fused[0].id, "doc2");
    }

    #[test]
    fn test_original_query_weight() {
        let mut fusion = RRFFusion::new();

        // 原始查詢權重 ×2
        fusion.add_results(
            "original",
            vec![RankedResult {
                id: "doc1".to_string(),
                rank: 0,
            }],
            2.0,
        );

        // 擴展查詢權重 ×1
        fusion.add_results(
            "expanded",
            vec![RankedResult {
                id: "doc2".to_string(),
                rank: 0,
            }],
            1.0,
        );

        let fused = fusion.fuse(10);
        // doc1 應該排第一（權重更高）
        assert_eq!(fused[0].id, "doc1");
        assert!(fused[0].score > fused[1].score);
    }

    #[test]
    fn test_top_rank_bonus() {
        let mut fusion = RRFFusion::new();

        fusion.add_results(
            "engine",
            vec![
                RankedResult {
                    id: "doc1".to_string(),
                    rank: 0,
                }, // 應該得到 +0.05 bonus
                RankedResult {
                    id: "doc2".to_string(),
                    rank: 1,
                }, // 應該得到 +0.03 bonus
                RankedResult {
                    id: "doc3".to_string(),
                    rank: 2,
                }, // 無 bonus
            ],
            1.0,
        );

        let fused = fusion.fuse(10);
        assert_eq!(fused[0].id, "doc1");
        assert_eq!(fused[1].id, "doc2");
        assert_eq!(fused[2].id, "doc3");

        // 驗證 bonus 有被加上
        assert!(fused[0].score > 1.0 / 61.0 + 0.04); // 基礎分數 + bonus
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
                RankedResult {
                    id: "doc1".to_string(),
                    rank: 0,
                },
                RankedResult {
                    id: "doc2".to_string(),
                    rank: 1,
                },
                RankedResult {
                    id: "doc3".to_string(),
                    rank: 2,
                },
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

        fusion.add_results(
            "engine",
            vec![RankedResult {
                id: "doc1".to_string(),
                rank: 0,
            }],
            1.0,
        );

        let fused = fusion.fuse(1);
        // 使用 k=30 時，分數應該是 1/(30+0+1) = 1/31 ≈ 0.0323
        // 加上 rank 0 的 bonus 0.05
        assert!((fused[0].score - (1.0 / 31.0 + 0.05)).abs() < 0.001);
    }
}

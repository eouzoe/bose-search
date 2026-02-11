/// 位置感知混合 (Position-Aware Blending)
///
/// 根據排名動態調整 RRF 分數和重排序分數的權重。
/// 保護 top 結果的檢索信號，同時利用模型改善長尾。
///
/// # Algorithm
///
/// ```text
/// rank 1-3:   75% RRF + 25% reranker  (保護精確匹配)
/// rank 4-10:  50% RRF + 50% reranker  (平衡)
/// rank 11+:   25% RRF + 75% reranker  (信任模型)
/// ```
///
/// # Examples
///
/// ```
/// use bose_common::blending::blend_scores;
///
/// // Top 結果：保護檢索信號
/// let score = blend_scores(0.8, 0.6, 1);
/// assert!((score - 0.75).abs() < 0.01); // 0.75 * 0.8 + 0.25 * 0.6
///
/// // 中間結果：平衡
/// let score = blend_scores(0.7, 0.8, 5);
/// assert!((score - 0.75).abs() < 0.01); // 0.5 * 0.7 + 0.5 * 0.8
///
/// // 長尾結果：信任模型
/// let score = blend_scores(0.5, 0.9, 15);
/// assert!((score - 0.8).abs() < 0.01); // 0.25 * 0.5 + 0.75 * 0.9
/// ```
pub fn blend_scores(rrf_score: f64, reranker_score: f64, rank: usize) -> f64 {
    let (rrf_weight, reranker_weight) = get_weights(rank);
    rrf_weight * rrf_score + reranker_weight * reranker_score
}

/// 根據排名獲取權重
///
/// # Returns
///
/// 返回 (rrf_weight, reranker_weight)
fn get_weights(rank: usize) -> (f64, f64) {
    match rank {
        1..=3 => (0.75, 0.25),  // 保護 top 結果
        4..=10 => (0.50, 0.50), // 平衡
        _ => (0.25, 0.75),      // 信任模型
    }
}

/// 批量混合分數
///
/// # Arguments
///
/// * `results` - 結果列表，包含 RRF 分數和重排序分數
///
/// # Returns
///
/// 返回混合後的結果列表
pub fn blend_results(results: Vec<BlendInput>) -> Vec<BlendOutput> {
    results
        .into_iter()
        .enumerate()
        .map(|(idx, input)| {
            let rank = idx + 1; // 1-based rank
            let blended_score = blend_scores(input.rrf_score, input.reranker_score, rank);

            BlendOutput {
                id: input.id,
                blended_score,
                rrf_score: input.rrf_score,
                reranker_score: input.reranker_score,
                rank,
            }
        })
        .collect()
}

/// 混合輸入
#[derive(Debug, Clone)]
pub struct BlendInput {
    /// 文檔 ID
    pub id: String,
    /// RRF 分數
    pub rrf_score: f64,
    /// 重排序分數
    pub reranker_score: f64,
}

/// 混合輸出
#[derive(Debug, Clone, PartialEq)]
pub struct BlendOutput {
    /// 文檔 ID
    pub id: String,
    /// 混合後的分數
    pub blended_score: f64,
    /// 原始 RRF 分數
    pub rrf_score: f64,
    /// 原始重排序分數
    pub reranker_score: f64,
    /// 排名（1-based）
    pub rank: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_rank_weights() {
        // Rank 1-3: 75% RRF + 25% reranker
        let score = blend_scores(0.8, 0.6, 1);
        assert!((score - 0.75).abs() < 0.001); // 0.75 * 0.8 + 0.25 * 0.6 = 0.75

        let score = blend_scores(0.8, 0.6, 3);
        assert!((score - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_middle_rank_weights() {
        // Rank 4-10: 50% RRF + 50% reranker
        let score = blend_scores(0.7, 0.8, 5);
        assert!((score - 0.75).abs() < 0.001); // 0.5 * 0.7 + 0.5 * 0.8 = 0.75

        let score = blend_scores(0.6, 0.9, 10);
        assert!((score - 0.75).abs() < 0.001); // 0.5 * 0.6 + 0.5 * 0.9 = 0.75
    }

    #[test]
    fn test_long_tail_weights() {
        // Rank 11+: 25% RRF + 75% reranker
        let score = blend_scores(0.5, 0.9, 15);
        assert!((score - 0.8).abs() < 0.001); // 0.25 * 0.5 + 0.75 * 0.9 = 0.8

        let score = blend_scores(0.4, 0.8, 100);
        assert!((score - 0.7).abs() < 0.001); // 0.25 * 0.4 + 0.75 * 0.8 = 0.7
    }

    #[test]
    fn test_blend_results() {
        let inputs = vec![
            BlendInput {
                id: "doc1".to_string(),
                rrf_score: 0.9,
                reranker_score: 0.7,
            },
            BlendInput {
                id: "doc2".to_string(),
                rrf_score: 0.8,
                reranker_score: 0.85,
            },
            BlendInput {
                id: "doc3".to_string(),
                rrf_score: 0.6,
                reranker_score: 0.95,
            },
        ];

        let outputs = blend_results(inputs);

        assert_eq!(outputs.len(), 3);

        // doc1 (rank 1): 0.75 * 0.9 + 0.25 * 0.7 = 0.85
        assert_eq!(outputs[0].id, "doc1");
        assert_eq!(outputs[0].rank, 1);
        assert!((outputs[0].blended_score - 0.85).abs() < 0.001);

        // doc2 (rank 2): 0.75 * 0.8 + 0.25 * 0.85 = 0.8125
        assert_eq!(outputs[1].id, "doc2");
        assert_eq!(outputs[1].rank, 2);
        assert!((outputs[1].blended_score - 0.8125).abs() < 0.001);

        // doc3 (rank 3): 0.75 * 0.6 + 0.25 * 0.95 = 0.6875
        assert_eq!(outputs[2].id, "doc3");
        assert_eq!(outputs[2].rank, 3);
        assert!((outputs[2].blended_score - 0.6875).abs() < 0.001);
    }

    #[test]
    fn test_empty_results() {
        let outputs = blend_results(vec![]);
        assert_eq!(outputs.len(), 0);
    }

    #[test]
    fn test_boundary_ranks() {
        // Rank 3 (top boundary)
        let score = blend_scores(1.0, 0.0, 3);
        assert!((score - 0.75).abs() < 0.001);

        // Rank 4 (middle start)
        let score = blend_scores(1.0, 0.0, 4);
        assert!((score - 0.50).abs() < 0.001);

        // Rank 10 (middle end)
        let score = blend_scores(1.0, 0.0, 10);
        assert!((score - 0.50).abs() < 0.001);

        // Rank 11 (long tail start)
        let score = blend_scores(1.0, 0.0, 11);
        assert!((score - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_reranker_improves_long_tail() {
        // 長尾結果：重排序模型發現了好文檔
        let rrf_score = 0.3; // RRF 認為不太相關
        let reranker_score = 0.95; // 但重排序模型認為很相關
        let rank = 20;

        let blended = blend_scores(rrf_score, reranker_score, rank);
        // 0.25 * 0.3 + 0.75 * 0.95 = 0.7875
        assert!((blended - 0.7875).abs() < 0.001);

        // 混合後的分數更接近重排序分數
        assert!((blended - reranker_score).abs() < (blended - rrf_score).abs());
    }

    #[test]
    fn test_protect_top_results() {
        // Top 結果：保護檢索信號
        let rrf_score = 0.95; // RRF 認為很相關（精確匹配）
        let reranker_score = 0.6; // 但重排序模型誤判
        let rank = 1;

        let blended = blend_scores(rrf_score, reranker_score, rank);
        // 0.75 * 0.95 + 0.25 * 0.6 = 0.8625
        assert!((blended - 0.8625).abs() < 0.001);

        // 混合後的分數更接近 RRF 分數
        assert!((blended - rrf_score).abs() < (blended - reranker_score).abs());
    }
}

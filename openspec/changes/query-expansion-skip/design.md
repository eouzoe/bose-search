# Design: Query Expansion Skip Logic

## Architecture

### Module Structure

```
crates/bose-mcp/src/
├── router.rs          # 新增：查詢擴展路由器
├── tools.rs           # 修改：整合路由邏輯
└── lib.rs             # 修改：匯出 router 模組
```

### Data Flow

```
User Query
    │
    ▼
web_search tool
    │
    ▼
SearXNG Client ──> Raw Results
    │
    ▼
QueryExpansionRouter.should_skip_expansion()
    │
    ├─ Skip? ──> Return Results (fast path)
    │
    └─ Expand? ──> Query Expansion Service ──> Enhanced Results
```

## Implementation Details

### 1. Router Module (`router.rs`)

```rust
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 查詢擴展路由器配置
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
    /// 從環境變數建立
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
    pub fn should_skip_expansion(&self, results: &[SearchResult]) -> SkipDecision {
        if results.is_empty() {
            return SkipDecision {
                skip: false,
                reason: "No results, expansion may help".to_string(),
                top_score: None,
                gap: None,
            };
        }

        let top_score = results[0].score;

        // 計算 gap
        let gap = if results.len() > 1 {
            top_score - results[1].score
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkipDecision {
    /// 是否跳過擴展
    pub skip: bool,
    /// 決策原因
    pub reason: String,
    /// Top score（如果有結果）
    pub top_score: Option<f64>,
    /// Score gap（如果有結果）
    pub gap: Option<f64>,
}

/// 搜尋結果（簡化版，實際使用 bose-common 的類型）
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: f64,
    // ... 其他欄位
}
```

### 2. Integration (`tools.rs`)

```rust
use crate::router::QueryExpansionRouter;

// 在 web_search tool 中整合
pub async fn web_search(query: String, ...) -> Result<SearchResponse> {
    // 1. 執行搜尋
    let results = searxng_client.search(&query).await?;

    // 2. 決定是否擴展
    let router = QueryExpansionRouter::from_env();
    let decision = router.should_skip_expansion(&results);

    // 3. 根據決策返回結果
    if decision.skip {
        Ok(SearchResponse {
            results,
            expanded: false,
            decision_reason: Some(decision.reason),
        })
    } else {
        // 執行查詢擴展（未來實作）
        Ok(SearchResponse {
            results,
            expanded: false, // TODO: 實作擴展邏輯
            decision_reason: Some(decision.reason),
        })
    }
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_high_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            SearchResult { score: 0.92 },
            SearchResult { score: 0.65 },
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(decision.skip);
        assert_eq!(decision.top_score, Some(0.92));
        assert_eq!(decision.gap, Some(0.27));
    }

    #[test]
    fn test_expand_low_confidence() {
        let router = QueryExpansionRouter::default();
        let results = vec![
            SearchResult { score: 0.78 },
            SearchResult { score: 0.72 },
        ];

        let decision = router.should_skip_expansion(&results);
        assert!(!decision.skip);
    }

    // ... 更多測試
}
```

### Property Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_threshold_boundaries(
        top_score in 0.0..=1.0,
        second_score in 0.0..=1.0,
    ) {
        let router = QueryExpansionRouter::default();
        let results = vec![
            SearchResult { score: top_score.max(second_score) },
            SearchResult { score: top_score.min(second_score) },
        ];

        let decision = router.should_skip_expansion(&results);

        // 驗證決策邏輯一致性
        let expected_skip = top_score >= 0.85 && (top_score - second_score) >= 0.15;
        assert_eq!(decision.skip, expected_skip);
    }
}
```

## Configuration

### Environment Variables

```bash
# 可選配置
export QUERY_EXPANSION_SKIP_THRESHOLD=0.85  # Top score 閾值
export QUERY_EXPANSION_GAP_THRESHOLD=0.15   # Gap 閾值
```

## Rollout Plan

### Phase 1: 實作核心邏輯
- [ ] 建立 `router.rs` 模組
- [ ] 實作 `QueryExpansionRouter`
- [ ] 單元測試

### Phase 2: 整合
- [ ] 修改 `tools.rs` 整合路由
- [ ] 整合測試

### Phase 3: 驗證
- [ ] 屬性測試
- [ ] 效能測試
- [ ] 文檔

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| 閾值不適合實際場景 | 中 | 可透過環境變數調整 |
| 決策邏輯有 bug | 高 | 完整的單元測試 + 屬性測試 |
| 效能影響 | 低 | 簡單的數值比較，<1ms |

## Future Enhancements

1. **動態閾值調整** - 根據歷史數據自動調整
2. **A/B 測試框架** - 比較不同閾值的效果
3. **監控儀表板** - 追蹤跳過率、準確率

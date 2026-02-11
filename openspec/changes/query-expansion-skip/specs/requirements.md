# Specification: Query Expansion Skip Logic

## Requirements

### Functional Requirements

**FR1: 置信度評估**
- 系統必須能夠評估搜尋結果的置信度
- 置信度基於 top score 和 score gap

**FR2: 跳過決策**
- 當 top_score ≥ 0.85 且 gap ≥ 0.15 時，跳過查詢擴展
- 否則，執行查詢擴展

**FR3: 結果返回**
- 跳過擴展時，直接返回原始搜尋結果
- 執行擴展時，返回擴展後的結果

### Non-Functional Requirements

**NFR1: 效能**
- 決策邏輯必須在 <1ms 內完成
- 不增加額外的網路請求

**NFR2: 可測試性**
- 所有決策路徑必須可測試
- 提供屬性測試驗證邊界條件

**NFR3: 可維護性**
- 閾值可配置（透過環境變數）
- 清晰的日誌記錄決策原因

## Scenarios

### Scenario 1: 高置信度結果（跳過擴展）

**Given**: 搜尋查詢 "Rust async programming"
**When**:
- Top result score = 0.92
- Second result score = 0.65
- Gap = 0.27

**Then**:
- 決策 = 跳過擴展
- 返回原始結果
- 日誌記錄: "Skipping expansion: high confidence (score=0.92, gap=0.27)"

### Scenario 2: 低置信度結果（執行擴展）

**Given**: 搜尋查詢 "best practices"
**When**:
- Top result score = 0.78
- Second result score = 0.72
- Gap = 0.06

**Then**:
- 決策 = 執行擴展
- 呼叫查詢擴展服務
- 返回擴展後的結果

### Scenario 3: 邊界條件（top score 剛好 0.85）

**Given**: 搜尋查詢 "example query"
**When**:
- Top result score = 0.85
- Second result score = 0.68
- Gap = 0.17

**Then**:
- 決策 = 跳過擴展（≥ 0.85）
- 返回原始結果

### Scenario 4: 只有一個結果

**Given**: 搜尋查詢返回單一結果
**When**:
- Top result score = 0.90
- No second result

**Then**:
- Gap = 1.0 (假設 second = 0)
- 決策 = 跳過擴展
- 返回原始結果

### Scenario 5: 空結果

**Given**: 搜尋查詢返回 0 個結果
**When**: Results = []

**Then**:
- 決策 = 執行擴展（嘗試改善查詢）
- 呼叫查詢擴展服務

## API Design

```rust
/// 查詢擴展路由器
pub struct QueryExpansionRouter {
    skip_threshold: f64,  // 預設 0.85
    gap_threshold: f64,   // 預設 0.15
}

impl QueryExpansionRouter {
    /// 決定是否跳過查詢擴展
    pub fn should_skip_expansion(&self, results: &[SearchResult]) -> SkipDecision {
        // 實作邏輯
    }
}

/// 跳過決策
pub struct SkipDecision {
    pub skip: bool,
    pub reason: String,
    pub top_score: Option<f64>,
    pub gap: Option<f64>,
}
```

## Acceptance Criteria

- [ ] 所有 5 個 scenario 的測試通過
- [ ] 屬性測試驗證閾值邊界
- [ ] 決策邏輯 <1ms
- [ ] 日誌記錄決策原因
- [ ] 閾值可透過環境變數配置
- [ ] Clippy 零警告
- [ ] 文檔完整（包含範例）

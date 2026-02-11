# Proposal: Query Expansion Skip Logic

## Why

當搜尋結果的 top score 足夠高且與第二名差距夠大時，表示已經找到高度相關的結果，不需要執行昂貴的查詢擴展操作。這可以：

1. **降低延遲** - 跳過 LLM 查詢擴展（通常需要 100-500ms）
2. **節省成本** - 避免不必要的 API 呼叫
3. **提升用戶體驗** - 快速返回高品質結果

## What

實作查詢擴展跳過邏輯，根據搜尋結果的置信度決定是否需要查詢擴展。

### 決策規則

```rust
if top_score >= 0.85 && (top_score - second_score) >= 0.15 {
    // 跳過查詢擴展，直接返回結果
} else {
    // 執行查詢擴展以改善結果
}
```

### 範例場景

**場景 1: 跳過擴展**
- Top score: 0.92
- Second score: 0.65
- Gap: 0.27 (≥ 0.15) ✓
- 決策: 跳過擴展

**場景 2: 執行擴展**
- Top score: 0.78
- Second score: 0.72
- Gap: 0.06 (< 0.15) ✗
- 決策: 執行擴展

## Impact

### 效能影響
- **最佳情況**: 延遲降低 100-500ms (跳過 LLM 呼叫)
- **最差情況**: 無影響 (仍執行擴展)
- **預期**: 約 30-40% 查詢可跳過擴展

### 程式碼影響
- 新增模組: `crates/bose-mcp/src/router.rs`
- 修改: `crates/bose-mcp/src/tools.rs` (整合路由邏輯)
- 測試: 單元測試 + 屬性測試

## Alternatives Considered

1. **固定閾值 (只看 top score)** - 不考慮 gap，可能誤判
2. **機器學習模型** - 過度複雜，不符合零成本目標
3. **不實作** - 浪費資源，用戶體驗差

## References

- QMD 架構分析: `reports/qmd-architecture-analysis.md`
- Beads 任務: `bose-search-orj`

# Proposal: Reciprocal Rank Fusion (RRF)

## Why

當使用多個搜尋引擎或多種搜尋模式（BM25、向量搜尋等）時，需要一個有效的方法來合併結果。RRF 是一個簡單但強大的融合演算法：

1. **無需分數校準** - 只看排名，不需要標準化不同引擎的分數
2. **對異常值魯棒** - 不會被單一引擎的極端分數影響
3. **簡單高效** - 計算複雜度 O(n)，無需機器學習模型

## What

實作 Reciprocal Rank Fusion 演算法，用於合併多個搜尋引擎的結果。

### 核心公式

```rust
RRF_score = weight / (k + rank + 1)
```

其中：
- `k = 60` (常數，來自論文 Cormack et al. 2009)
- `rank` 是結果在該引擎中的排名（0-based）
- `weight` 是引擎權重（原始查詢權重 ×2）

### 特殊處理

1. **原始查詢權重加倍** - 保護精確匹配
2. **Top-rank 加成** - 防止高排名結果被稀釋
   - rank 0: +0.05
   - rank 1: +0.03

### 範例

**輸入**:
- Engine A: [doc1(rank=0), doc2(rank=1), doc3(rank=2)]
- Engine B: [doc2(rank=0), doc4(rank=1), doc1(rank=3)]

**計算**:
- doc1: 1/(60+0+1) + 1/(60+3+1) = 0.0164 + 0.0156 = 0.032
- doc2: 1/(60+1+1) + 1/(60+0+1) = 0.0161 + 0.0164 = 0.0325
- doc3: 1/(60+2+1) = 0.0159
- doc4: 1/(60+1+1) = 0.0161

**輸出**: [doc2, doc1, doc4, doc3]

## Impact

### 效能影響
- **計算複雜度**: O(n) where n = 總結果數
- **記憶體**: O(n) for HashMap
- **延遲**: <1ms for 100 results

### 程式碼影響
- 新增模組: `crates/bose-common/src/fusion.rs`
- 測試: 單元測試 + 屬性測試

## Alternatives Considered

1. **簡單分數加權** - 需要分數校準，不同引擎分數範圍不同
2. **CombSUM/CombMNZ** - 對異常值敏感
3. **機器學習模型** - 過度複雜，需要訓練數據

## References

- Cormack, G. V., Clarke, C. L., & Buettcher, S. (2009). Reciprocal rank fusion outperforms condorcet and individual rank learning methods.
- QMD 架構分析: `reports/qmd-architecture-analysis.md`
- Beads 任務: `bose-search-j39`

# Tasks: Query Expansion Skip Logic

## Phase 1: 核心實作 ⏱️ 20min

### Task 1.1: 建立 router 模組
- [ ] 建立 `crates/bose-mcp/src/router.rs`
- [ ] 定義 `QueryExpansionRouter` 結構
- [ ] 定義 `SkipDecision` 結構
- [ ] 實作 `Default` trait
- [ ] 實作 `from_env()` 方法

**驗收**: `cargo build -p bose-mcp` 成功

### Task 1.2: 實作決策邏輯
- [ ] 實作 `should_skip_expansion()` 方法
- [ ] 處理空結果情況
- [ ] 處理單一結果情況
- [ ] 處理多個結果情況
- [ ] 加入 tracing 日誌

**驗收**: 邏輯正確，日誌清晰

### Task 1.3: 單元測試
- [ ] 測試高置信度跳過（scenario 1）
- [ ] 測試低置信度擴展（scenario 2）
- [ ] 測試邊界條件（scenario 3）
- [ ] 測試單一結果（scenario 4）
- [ ] 測試空結果（scenario 5）

**驗收**: `cargo test -p bose-mcp router` 全部通過

## Phase 2: 整合 ⏱️ 10min

### Task 2.1: 匯出模組
- [ ] 在 `lib.rs` 中加入 `pub mod router;`
- [ ] 確認模組可見性

**驗收**: `cargo build -p bose-mcp` 成功

### Task 2.2: 更新 tools.rs（預留）
- [ ] 在 `web_search` 中加入 TODO 註解
- [ ] 說明未來如何整合路由邏輯

**驗收**: 註解清晰，不影響現有功能

## Phase 3: 進階測試 ⏱️ 10min

### Task 3.1: 屬性測試
- [ ] 加入 `proptest` 依賴
- [ ] 實作閾值邊界屬性測試
- [ ] 實作決策一致性屬性測試

**驗收**: `cargo test -p bose-mcp` 包含屬性測試

### Task 3.2: 文檔
- [ ] 為 `QueryExpansionRouter` 加入文檔註解
- [ ] 為 `should_skip_expansion` 加入範例
- [ ] 為 `SkipDecision` 加入說明

**驗收**: `cargo doc -p bose-mcp --open` 文檔完整

## Phase 4: 驗證 ⏱️ 5min

### Task 4.1: 最終檢查
- [ ] `cargo test --workspace` 全部通過
- [ ] `cargo clippy --workspace -- -D warnings` 零警告
- [ ] `cargo fmt --check` 格式正確
- [ ] 更新 Beads 任務狀態

**驗收**: 所有檢查通過

## 總計時間: ~45 分鐘

## 依賴關係

```
Task 1.1 → Task 1.2 → Task 1.3
                ↓
Task 2.1 → Task 2.2
                ↓
Task 3.1 → Task 3.2
                ↓
            Task 4.1
```

## 注意事項

1. **TDD 工作流**: 先寫測試，再寫實作
2. **小步提交**: 每個 task 完成後提交
3. **日誌記錄**: 使用 `tracing` 而非 `println!`
4. **錯誤處理**: 使用 `thiserror` 定義錯誤類型
5. **文檔優先**: 公開 API 必須有文檔註解

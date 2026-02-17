# Bose Search Engine — Progress

## 當前版本: v0.1.0 (已發布)

### 已完成
- [x] SearXNG MCP Server (web_search + health_check + local_search)
- [x] Tantivy 本地索引 (schema + indexer + searcher)
- [x] PID 背壓控制器 (bose-crawler)
- [x] reqwest fetcher (bose-crawler)
- [x] 155 tests pass
- [x] v0.1.0 GitHub release

### v0.2 進行中
- [x] read_url MCP tool (F001 完成)
- [x] Moka LRU cache (F002 完成)
- [x] crawl_knowledge MCP tool (F003 完成)
- [x] lefthook hooks 啟用 (F004 完成)
- [ ] GitHub Actions CI/CD
- [ ] trait SearchBackend plugin system

---

## Session Log

### Session #13 — 2026-02-18
**Agent**: Haiku 4.5 (執行 Agent)
**工作**: 配置 lefthook git hooks (F004)
- 安裝 lefthook v1.13.6 via Go
- 啟用 pre-commit, pre-push, commit-msg hooks
- 修復 clippy 警告（RangeInclusive::contains）
- 配置 hook 文件使用完整路徑（解決 PATH 問題）
- 驗證 hooks 正常工作：
  - ✔️ commit-msg 拒絕錯誤格式
  - ✔️ pre-commit 運行 fmt+clippy+typos
- 更新 CLAUDE.md 添加 PATH 配置說明
- Commit: 4803d2f
- 158 tests pass, clippy clean

### Session #12 — 2026-02-18
**Agent**: Sonnet 4.5 (執行 Agent)
**工作**: 實作 crawl_knowledge MCP tool (F003)
- 添加 CrawlKnowledgeParams struct（query, depth, source_filter）
- 實現完整 pipeline: SearXNG search → fetch URLs → extract text → index to Tantivy
- Depth 參數（1-10）控制抓取 URL 數量
- Source filter 允許按搜尋引擎過濾
- 返回索引文檔摘要
- 添加集成測試（2 文檔索引並可搜尋）
- 所有測試通過，clippy 零警告
- Commit: 8954c32

### Session #11 — 2026-02-18
**Agent**: Sonnet 4.5 (執行 Agent)
**工作**: 實作 Moka LRU cache (F002)
- 添加 moka 依賴（v0.12, future feature）
- 實現 make_cache_key() 函數（query+params → 字串）
- 在 BoseSearchServer 添加 Cache<String, SearchResponse>
- web_search 方法添加 cache hit/miss 邏輯和 tracing 日誌
- 添加 3 個測試（cache hit, TTL expiry, cache key）
- 所有測試通過，clippy 零警告
- Commit: 73b8900

### Session #10 — 2026-02-18
**Agent**: Sonnet 4.5 (執行 Agent)
**工作**: 實作 read_url MCP tool (F001)
- 添加 ReadUrlParams struct
- 實現 read_url 工具（reqwest + 10s timeout）
- 實現 strip_html_tags 函數（過濾 script/style 標籤）
- 添加 8 個測試（5 單元測試 + 3 集成測試）
- 所有測試通過，clippy 零警告
- Commit: 394e222

### Session #9 — 2026-02-18
**Agent**: Opus 4.6 (架構師)
**工作**: 建立 .harness/ 雙窗口任務隊列基礎設施
- 創建 feature_list.json, coding.md prompt, progress.md
- 關閉 3 個已完成的 beads tasks
- 審核錯誤報告 (Nix/cargo-outdated/budget 失敗)

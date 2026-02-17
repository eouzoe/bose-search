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
- [ ] read_url MCP tool
- [ ] Moka LRU cache
- [ ] crawl_knowledge MCP tool
- [ ] lefthook hooks 啟用
- [ ] GitHub Actions CI/CD
- [ ] trait SearchBackend plugin system

---

## Session Log

### Session #9 — 2026-02-18
**Agent**: Opus 4.6 (架構師)
**工作**: 建立 .harness/ 雙窗口任務隊列基礎設施
- 創建 feature_list.json, coding.md prompt, progress.md
- 關閉 3 個已完成的 beads tasks
- 審核錯誤報告 (Nix/cargo-outdated/budget 失敗)

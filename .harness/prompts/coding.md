# Coding Agent Prompt — Bose Search Engine

你是 Bose Search Engine 的執行 Agent。你的工作是從任務隊列中取出一個任務，實作它，測試它，提交它。

## 鐵則

1. **一次只做一個任務** — 不要嘗試一次完成多個功能
2. **先測試再實作** — TDD: 寫測試 → 失敗 → 實作 → 通過
3. **乾淨狀態** — 每次結束時代碼必須能編譯、測試通過、clippy 零警告
4. **不要改 feature_list.json 的描述** — 只能改 `"passes": false` → `"passes": true`
5. **搜尋工具**: bose-search (預設) → Exa (困難) → ⛔ 永不用 WebFetch/WebSearch
6. **程式庫文檔**: 用 Context7 MCP tool

## 開始工作 (每次 Session)

```bash
# 1. 了解環境
pwd
cat .harness/progress.md
git log --oneline -5

# 2. 啟動環境
bash init.sh  # 或手動: podman start bose-searxng

# 3. 驗證現有代碼
cargo test --workspace
cargo clippy -- -D warnings

# 4. 查看任務隊列
bd ready

# 5. 選擇一個任務 (優先 P1 > P2 > P3)
bd show <task-id>
bd update <task-id> --status=in_progress
```

## 實作流程

```
1. 讀取任務描述和驗收條件
2. 寫測試 (RED)
3. 運行測試 — 確認失敗
4. 寫最小實作 (GREEN)
5. 運行測試 — 確認通過
6. cargo clippy -- -D warnings
7. cargo fmt
8. git add <changed-files>
9. git commit -m "<type>(<scope>): <description>"
10. 更新 .harness/progress.md
11. 更新 .harness/feature_list.json (passes: true)
12. bd close <task-id> --reason="<驗收證據>"
```

## 結束 Session

```bash
# 必做 checklist
cargo test --workspace
cargo clippy -- -D warnings
cargo fmt --check
git status
bd sync
```

## 求助機制

如果遇到以下情況，**停止工作**並記錄：
- 測試失敗超過 3 次且無法修復
- 需要架構決策（新 crate、新依賴、API 變更）
- 編譯錯誤超過 3 次
- 不確定需求

```bash
# 記錄問題到 progress.md
# 標記任務為 blocked
bd update <task-id> --notes="BLOCKED: <問題描述>"
git commit -m "wip: blocked on <問題>"
```

## 專案結構

```
bose-search/
├── crates/
│   ├── bose-mcp/      # MCP Server (入口)
│   ├── bose-searxng/   # SearXNG 客戶端
│   ├── bose-index/     # Tantivy 本地索引
│   ├── bose-crawler/   # 爬蟲 + PID 控制器
│   └── bose-common/    # 共用類型
├── .harness/
│   ├── feature_list.json  # 功能清單 (只改 passes 欄位)
│   ├── progress.md        # 進度記錄
│   └── prompts/coding.md  # 本文件
├── init.sh               # 環境啟動
├── deny.toml             # cargo-deny 配置
└── lefthook.yml          # Git hooks
```

## 提交規範

```
<type>(<scope>): <subject>

type: feat|fix|docs|test|refactor|perf|chore
scope: mcp|searxng|index|crawler|common|harness
```

## 品質標準

- 測試覆蓋率 ≥ 80%
- clippy 零警告
- 函數 < 50 行
- 文件 < 800 行
- 錯誤處理: 公開 API 用 thiserror，內部用 anyhow
- 日誌: tracing (不用 println!)

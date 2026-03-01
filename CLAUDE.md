# CLAUDE.md — Bose Search Engine

> **每個 Agent 的第一件事就是讀這個文件。**

---

## 0. 專案概述

**Bose Search Engine** — 零成本、極致效能的 AI Agent 搜尋工具。
透過 MCP 協議暴露給 Claude Code，整合 SearXNG (247 引擎)。

- **語言**: Rust (stable 1.92+)
- **用戶語言**: 繁體中文
- **品質標準**: 對標 ripgrep / tantivy 等頂級 Rust 專案

### 搜尋工具 & 全域規範

搜尋工具優先級見 `~/.claude/rules/quality-baseline.md`（bose-search → Exa → Context7）。
全域代碼風格見 `~/.claude/rules/bose-style.md`。
雙窗口工作流見 `~/.claude/rules/workflow.md`。

---

## 1. 快速啟動

### 前置條件
- Rust 1.92+ (`rustup update stable`)
- Podman 4.9+ (`podman --version`)

### 啟動 SearXNG
```bash
cd deploy && podman-compose up -d
# 驗證: curl "http://localhost:8080/search?q=test&format=json"
```

### 編譯 & 測試
```bash
cargo build --release -p bose-mcp
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

### MCP Server 使用方式

#### 方式 1: 專案內自動載入
在 bose-search 目錄下工作時，Claude Code 會自動讀取 `.mcp.json`。

#### 方式 2: 全域註冊
```bash
claude mcp add bose-search \
  --scope user \
  -e SEARXNG_URL=http://localhost:8080 \
  -- /home/eouzoe/src/active/bose-search/target/release/bose-mcp
```

#### 方式 3: 手動測試
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocolVersion":"2025-03-26","capabilities":{},
  "clientInfo":{"name":"test","version":"0.1"}
}}' | cargo run -p bose-mcp 2>/dev/null
```

---

## 2. 架構

```
bose-search/
├── .mcp.json                    # MCP Server 配置
├── .claude/skills/web-search/   # Claude Code Skill
├── deploy/
│   ├── podman-compose.yml       # SearXNG 容器部署
│   └── searxng/settings.yml     # SearXNG 引擎配置
└── crates/
    ├── bose-common/             # 共用類型、錯誤、配置
    ├── bose-searxng/            # SearXNG HTTP 客戶端
    └── bose-mcp/                # MCP Server (rmcp 0.14)
```

### MCP Tools

| Tool | 說明 | 參數 |
|------|------|------|
| `web_search` | 搜尋網頁 (247 引擎) | query*, num_results, category, language, time_range |
| `health_check` | 檢查 SearXNG 狀態 | 無 |

---

## 3. 開發規範

- **格式化**: `cargo fmt`
- **Linting**: `cargo clippy -- -D warnings` (零警告)
- **測試**: `cargo test --workspace` (14 tests)
- **提交**: Conventional Commits (`feat|fix|docs|perf|test`)
- **容器**: Podman (不用 Docker)
- **TLS**: rustls (不用 OpenSSL)

---

## 4. 環境變數

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `SEARXNG_URL` | `http://localhost:8080` | SearXNG 服務地址 |
| `DEFAULT_NUM_RESULTS` | `10` | 預設搜尋結果數 |
| `REQUEST_TIMEOUT_SECS` | `30` | HTTP 請求超時 |

---

## 5. 開發工具鏈

### 必備工具

已安裝的 Rust 開發工具：

| 工具 | 用途 | 安裝命令 |
|------|------|----------|
| `cargo-audit` | 安全漏洞掃描 | `cargo install cargo-audit` |
| `cargo-deny` | 依賴許可證檢查 | `cargo install cargo-deny` |
| `cargo-machete` | 未使用依賴檢測 | `cargo install cargo-machete` |
| `typos` | 拼寫檢查 | `cargo install typos-cli` |
| `taplo` | TOML 格式化 | `cargo install taplo-cli` |
| `gitleaks` | Git 歷史密鑰掃描 | 見下方 |

### gitleaks 安裝

```bash
# 下載最新版本
curl -sSL https://api.github.com/repos/gitleaks/gitleaks/releases/latest | \
  grep "browser_download_url.*linux_x64.tar.gz" | \
  cut -d '"' -f 4 | \
  xargs curl -L -o /tmp/gitleaks.tar.gz

# 解壓並安裝
tar -xzf /tmp/gitleaks.tar.gz -C /tmp
mv /tmp/gitleaks ~/.local/bin/
chmod +x ~/.local/bin/gitleaks
```

### Git Hooks (lefthook)

專案使用 `lefthook` 管理 Git hooks。配置文件：`lefthook.yml`

**安裝 lefthook**:
```bash
# macOS
brew install lefthook

# Linux
curl -1sLf 'https://dl.cloudsmith.io/public/evilmartians/lefthook/setup.deb.sh' | sudo -E bash
sudo apt install lefthook

# 或使用 Go
go install github.com/evilmartians/lefthook@latest
```

**啟用 hooks**:
```bash
lefthook install

# 如果使用 Go 安裝，需要配置 PATH
# 方式 1: 修改 git hooks 使用完整路徑
sed -i 's|lefthook -h|$HOME/go/bin/lefthook -h|g' .git/hooks/*
sed -i 's|lefthook "$@"|$HOME/go/bin/lefthook "$@"|g' .git/hooks/*

# 方式 2: 添加到系統 PATH
echo 'export PATH="$HOME/go/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

**Hooks 內容**:
- **pre-commit**: cargo fmt, clippy, typos, taplo
- **pre-push**: cargo test, audit, deny, gitleaks
- **commit-msg**: Conventional Commits 格式檢查

### 初始化腳本

執行 `./init.sh` 自動檢查並設置開發環境：
- 啟動 SearXNG 容器
- 檢查 Rust 工具鏈
- 驗證開發工具
- 運行測試和安全掃描

### Exa API Key 配置

**⚠️ 重要**: Exa API key 必須使用環境變數，不可硬編碼到 `.mcp.json`。

**設置方式**:
```bash
# 添加到 ~/.zshrc 或 ~/.bashrc
export EXA_API_KEY="your-api-key-here"

# 重新載入配置
source ~/.zshrc
```

**`.mcp.json` 配置**:
```json
{
  "mcpServers": {
    "exa": {
      "type": "http",
      "url": "https://mcp.exa.ai/mcp?exaApiKey=${EXA_API_KEY}",
      "headers": {}
    }
  }
}
```

### Nix 包管理器 (可選)

**注意**: Nix 安裝需要 sudo 權限。如果不想使用 sudo，可以繼續使用 cargo install。

**安裝 Nix (多用戶模式)**:
```bash
curl -L https://nixos.org/nix/install | sh -s -- --daemon
```

安裝後需要重新登入或執行：
```bash
source /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
```

**使用 Nix 安裝工具**:
```bash
nix-env -iA nixpkgs.cargo-audit
nix-env -iA nixpkgs.cargo-deny
# ... 其他工具
```

## 5.5 雙窗口工作流

見 `~/.claude/rules/workflow.md`（全域）+ `~/src/active/.harness/prompts/coding-agent.xml`。

專案特有：啟動前先跑 `bash init.sh` 確保 SearXNG 容器運行。

```bash
# 啟動執行窗口
cd ~/src/active/bose-search && claude --model sonnet --dangerously-skip-permissions -p \
  "讀取 @CLAUDE.md @.harness/feature_list.json @.harness/progress.md @~/src/active/.harness/prompts/coding-agent.xml 然後 bd ready 開始工作"
```

---

## 6. MCP 工具整合

### 已整合的 MCP Servers

#### 1. Bose Search (本專案)
- **功能**: 網頁搜尋 (SearXNG 247 引擎)
- **Tools**: `web_search`, `health_check`
- **配置**: 見 `.mcp.json`

#### 2. Context7 (Upstash)
- **功能**: 即時程式庫文檔查詢
- **用途**: 獲取最新、版本特定的 API 文檔和代碼範例
- **使用方式**: 在 prompt 中加入 `use context7`
- **範例**:
  ```
  How do I use tokio::spawn to run background tasks? use context7
  Create a Next.js 14 API route with middleware. use context7
  ```
- **註冊命令**: `claude mcp add context7 --scope user -- bunx @upstash/context7-mcp@latest`
- **官網**: https://context7.com
- **GitHub**: https://github.com/upstash/context7

#### 3. Exa (搜尋與代碼查詢)
- **功能**: 網頁搜尋、公司研究、代碼文檔查詢
- **Tools**:
  - `web_search_exa` — 通用網頁搜尋
  - `company_research_exa` — 公司資訊研究
  - `get_code_context_exa` — GitHub/Stack Overflow/官方文檔搜尋
- **用途**: 研究技術文檔、查找代碼範例、了解公司產品

### 最佳實踐

**自動觸發 Context7** — 在本文件加入規則：
```
Always use Context7 MCP when I need library/API documentation or code examples.
Automatically add "use context7" to prompts about:
- Rust crates (tokio, serde, reqwest, etc.)
- Web frameworks (Next.js, React, Express, etc.)
- Databases (MongoDB, PostgreSQL, Redis, etc.)
```

**指定版本**:
```
How do I use Next.js 14 app router? use context7
Show me Rust tokio 1.35 spawn examples. use context7
```

**多庫查詢**:
```
Create a React component with TailwindCSS and fetch data from Supabase.
use context7 for react, tailwindcss, supabase
```

---

## 6. 文檔索引

| 文檔 | 位置 | 說明 |
|------|------|------|
| CLAUDE.md | 本文件 | 專案規範 (必讀) |
| .mcp.json | 專案根目錄 | MCP Server 配置 |
| SKILL.md | .claude/skills/web-search/ | Claude Code Skill 定義 |
| podman-compose.yml | deploy/ | SearXNG 部署配置 |
| settings.yml | deploy/searxng/ | SearXNG 引擎權重配置 |

---

## 3. QMD 演算法移植計劃

以下演算法從 tobi/qmd 專案借鑑，需用 Rust 重新實作。
詳細分析見 `reports/qmd-architecture-analysis.md`。

### 已移植
（無）

### 待移植（Phase 2 開發時由 Sonnet 執行）

| 演算法 | 目標檔案 | 優先級 | 工作量 |
|--------|---------|--------|--------|
| 查詢擴展跳過 | `crates/bose-mcp/src/router.rs` | P0 | 30min |
| RRF 融合 | `crates/bose-common/src/fusion.rs` | P1 | 3-4h |
| 位置感知混合 | `crates/bose-common/src/blending.rs` | P1 | 1-2h |
| 內容去重 | `crates/bose-common/src/dedup.rs` | P1 | 2-3h |
| 文檔分塊 | `crates/bose-common/src/chunker.rs` | P2 | 4-5h |
| MCP 結構化回應 | `crates/bose-mcp/src/tools.rs` | P2 | 1h |

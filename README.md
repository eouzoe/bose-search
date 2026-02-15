# Bose Search

Zero-cost, high-performance search for AI agents via [Model Context Protocol](https://modelcontextprotocol.io/).

Bose aggregates 247 search engines through [SearXNG](https://docs.searxng.org/) and maintains a local knowledge index powered by [Tantivy](https://github.com/quickwit-oss/tantivy) — exposed as MCP tools that any compatible client can use straightaway.

## Why

Paid search APIs burn through credits remarkably fast. Built-in LLM search is slow and unreliable. Bose gives your AI agent access to the entire web and a persistent local knowledge base, at no cost whatsoever.

- **247 engines** — Google, Brave, DuckDuckGo, Startpage, and 243 more via SearXNG
- **Local index** — Sub-10ms full-text search over crawled content via Tantivy
- **MCP native** — Works with Claude Code, Cursor, Windsurf, and any MCP-compatible client
- **Self-hosted** — Your queries never leave your machine
- **Single binary** — One 8 MB Rust executable, minimal resource footprint

## Quick Start

**Prerequisites:** [Rust](https://rustup.rs/) 1.82+ and [Podman](https://podman.io/) 4.0+ (or Docker).

```bash
# 1. Install
cargo install --git https://github.com/eouzoe/bose-search.git bose-search

# 2. Start SearXNG
podman run -d -p 8080:8080 --name searxng docker.io/searxng/searxng:latest

# 3. Register with Claude Code
claude mcp add bose-search \
  -e SEARXNG_URL=http://localhost:8080 \
  -- bose-search
```

Your AI agent now has access to 247 search engines.

## Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `web_search` | Query the web via 247 engines | `query`, `num_results`, `category`, `language`, `time_range` |
| `local_search` | Query the local knowledge index (<10ms) | `query`, `num_results` |
| `health_check` | Verify SearXNG connectivity | — |

## Architecture

```
crates/
├── bose-mcp/        MCP server entry point (rmcp 0.14)
├── bose-searxng/    SearXNG HTTP client
├── bose-common/     Shared types, configuration, algorithms
├── bose-index/      Tantivy local index
└── bose-crawler/    Content fetcher with PID backpressure control
```

> **Note**: `bose-crawler` is included as an experimental crate. It provides PID-controlled rate limiting and HTTP fetching primitives but is not yet integrated into the main binary.

## Configuration

All settings are read from environment variables.

| Variable | Default | Description |
|----------|---------|-------------|
| `SEARXNG_URL` | `http://localhost:8080` | SearXNG instance URL |
| `DEFAULT_NUM_RESULTS` | `10` | Results per query |
| `REQUEST_TIMEOUT_SECS` | `30` | HTTP request timeout in seconds |
| `BOSE_INDEX_PATH` | `$XDG_DATA_HOME/bose-search/index` | Local index directory |

## Building from Source

```bash
git clone https://github.com/eouzoe/bose-search.git
cd bose-search
cargo build --release -p bose-search
```

The binary will be at `target/release/bose-search`.

## Roadmap

- [x] SearXNG meta-search integration (247 engines)
- [x] MCP server with structured tool responses
- [x] Local Tantivy knowledge index with full-text search
- [x] Content crawler with PID-controlled backpressure
- [ ] Streaming hybrid results — local results first, web results as they arrive
- [ ] Plugin system for custom search backends
- [ ] SIMD-accelerated BM25 scoring engine
- [ ] io_uring async I/O via monoio

## Licence

Licensed under either of [Apache Licence, Version 2.0](LICENSE-APACHE) or [MIT Licence](LICENSE-MIT) at your option.

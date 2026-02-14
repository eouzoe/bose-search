# Bose Search

Zero-cost, high-performance search for AI agents via [Model Context Protocol](https://modelcontextprotocol.io/).

Bose aggregates 247 search engines through SearXNG and maintains a local knowledge index powered by Tantivy — all exposed as MCP tools that any compatible AI client can use out of the box.

## Why

Paid search APIs burn through credits fast. Built-in LLM search is slow and unreliable. Bose gives your AI agent access to the entire web and a persistent local knowledge base, for free.

- **247 engines** — Google, Brave, DuckDuckGo, Startpage, and 243 more via SearXNG
- **Local index** — Sub-10ms full-text search over crawled content (Tantivy)
- **MCP native** — Works with Claude Code, Cursor, Windsurf, and any MCP client
- **Self-hosted** — Your queries never leave your machine
- **Rust** — Single 8MB binary, minimal resource footprint

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.82+
- [Podman](https://podman.io/) 4.0+ (or Docker)

### 1. Install

```bash
cargo install bose-search
```

### 2. Start SearXNG

```bash
# Using the bundled compose file
cd deploy && podman-compose up -d

# Or pull directly
podman run -d -p 8080:8080 --name searxng docker.io/searxng/searxng:latest
```

### 3. Register with your AI client

```bash
# Claude Code
claude mcp add bose-search \
  -e SEARXNG_URL=http://localhost:8080 \
  -- bose-search

# Or add to your MCP config manually
```

That's it. Your AI agent now has access to 247 search engines.

## MCP Tools

| Tool | Description | Key Parameters |
|------|-------------|----------------|
| `web_search` | Search the web via 247 engines | `query`, `num_results`, `category`, `language`, `time_range` |
| `local_search` | Query the local knowledge index (<10ms) | `query`, `num_results` |
| `health_check` | Verify SearXNG connectivity | — |

## Architecture

```
bose-search/
├── crates/
│   ├── bose-mcp/        # MCP server (rmcp 0.14)
│   ├── bose-searxng/    # SearXNG HTTP client
│   ├── bose-common/     # Shared types, config, algorithms
│   ├── bose-index/      # Tantivy local index
│   └── bose-crawler/    # Content fetcher with backpressure
└── deploy/
    └── podman-compose.yml
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SEARXNG_URL` | `http://localhost:8080` | SearXNG instance URL |
| `DEFAULT_NUM_RESULTS` | `10` | Results per query |
| `REQUEST_TIMEOUT_SECS` | `30` | HTTP timeout |
| `BOSE_INDEX_PATH` | `$XDG_DATA_HOME/bose-search/index` | Local index directory |

## Roadmap

- [x] SearXNG meta-search (247 engines)
- [x] MCP server with web_search + health_check
- [x] Local Tantivy knowledge index
- [x] Content crawler with PID backpressure
- [ ] Streaming hybrid results (local + web)
- [ ] Plugin system for custom search backends
- [ ] SIMD-accelerated BM25 scoring
- [ ] io_uring async I/O (monoio)

## Building from Source

```bash
git clone https://github.com/eouzoe/bose-search.git
cd bose-search
cargo build --release -p bose-mcp
```

The binary is at `target/release/bose-mcp`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.

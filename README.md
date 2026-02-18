# Bose Search

[![CI](https://github.com/eouzoe/bose-search/actions/workflows/ci.yml/badge.svg)](https://github.com/eouzoe/bose-search/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](https://www.rust-lang.org/)

Zero-cost, high-performance search for AI agents via [Model Context Protocol](https://modelcontextprotocol.io/).

Bose aggregates 247 search engines through [SearXNG](https://docs.searxng.org/) and maintains a local knowledge index powered by [Tantivy](https://github.com/quickwit-oss/tantivy) — exposed as MCP tools that any compatible client can use straightaway.

## Why

Paid search APIs burn through credits remarkably fast. Built-in LLM search is slow and unreliable. Bose gives your AI agent access to the entire web and a persistent local knowledge base, at no cost whatsoever.

- **247 engines** — Google, Brave, DuckDuckGo, Startpage, and 243 more via SearXNG
- **Local index** — Sub-10ms full-text search over crawled content via Tantivy
- **MCP native** — Works with Claude Code, Cursor, Windsurf, and any MCP-compatible client
- **Self-hosted** — Your queries never leave your machine
- **Single binary** — One Rust executable, minimal resource footprint

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
├── bose-search/     MCP server entry point (rmcp 0.14)
├── bose-searxng/    SearXNG HTTP client
├── bose-common/     Shared types, configuration, algorithms
├── bose-index/      Tantivy local index
└── bose-crawler/    Content fetcher with PID backpressure control
```

> **Note**: `bose-crawler` is included as an experimental crate. It provides PID-controlled rate limiting and HTTP fetching primitives but is not yet integrated into the main binary.

## Algorithms

Bose implements several ranking and processing algorithms:

- **RRF Fusion** (`bose-common/fusion.rs`) — Reciprocal Rank Fusion merges ranked lists from multiple search engines into a single ordering, without needing score calibration across sources.
- **Position-aware Blending** (`bose-common/blending.rs`) — Interleaves results from different backends while preserving positional relevance signals.
- **PID Backpressure** (`bose-crawler/pid.rs`) — A proportional-integral-derivative controller that dynamically adjusts crawl rate based on server response latency, preventing overload.
- **SHA-256 Deduplication** (`bose-common/dedup.rs`) — Content-hash based dedup that eliminates duplicate results across engines.
- **Document Chunking** (`bose-common/chunker.rs`) — Splits large documents into indexable segments with configurable overlap for context preservation.
- **HTML Extraction** (`bose-common/extractor.rs`) — Strips markup to extract clean text content for indexing.
- **Query Expansion Router** (`bose-search/router.rs`) — Routes queries to the optimal combination of backends based on query characteristics.
- **Scavenger Algorithm** (`bose-common/scavenger.rs`) — Information-theoretic surprise scoring S(r) = −log₂(freq(r)/|E|) surfaces results that mainstream engines systematically miss. Greedy submodular selection with (1−1/e) approximation guarantee (Nemhauser et al. 1978) ensures diversity-aware top-k retrieval.

## SearXNG Deployment

Bose uses [SearXNG](https://docs.searxng.org/) as its meta-search backend. A `podman-compose.yml` is included in `deploy/`:

```bash
cd deploy && podman-compose up -d
```

This starts SearXNG on port 8080 with JSON API enabled and 4 default engines (Google, Brave, DuckDuckGo, Startpage). You can add more engines by editing `deploy/searxng/settings.yml`.

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

## Vision

Search engines surface what's popular, promoted, and paid for. Countless works — small projects, niche research, personal creations — get buried and forgotten over time.

Bose aims to change that. Beyond conventional search, we're building a "scavenging" algorithm inspired by biological decomposition and swarm intelligence: finding value in what others overlook. The long-term goal is to break through the filter bubble and help people discover content they'd never encounter through mainstream channels.

## Roadmap

- [x] SearXNG meta-search integration (247 engines)
- [x] MCP server with structured tool responses
- [x] Local Tantivy knowledge index with full-text search
- [x] Content crawler with PID-controlled backpressure
- [ ] Streaming hybrid results — local results first, web results as they arrive
- [x] Scavenging algorithm — core implementation complete (MCP integration pending)
- [ ] SIMD-accelerated BM25 scoring engine (Block-Max WAND)
- [ ] io_uring async I/O via monoio
- [ ] Plugin system for custom search backends

## Licence

Licensed under either of [Apache Licence, Version 2.0](LICENSE-APACHE) or [MIT Licence](LICENSE-MIT) at your option.

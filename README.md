# Bose Search

Zero-cost, high-performance search for AI agents via [Model Context Protocol](https://modelcontextprotocol.io/).

Bose aggregates search engines through [SearXNG](https://docs.searxng.org/) and maintains a local knowledge index powered by [Tantivy](https://github.com/quickwit-oss/tantivy) — exposed as MCP tools that any compatible client can use straightaway.

## Why

Paid search APIs burn through credits remarkably fast. Built-in LLM search is slow and unreliable. Bose gives your AI agent access to the open web and a persistent local knowledge base, at no cost whatsoever — and it **fetches each URL once**.

- **SearXNG meta-search** — one query fans out to a pool of engines; the response says which ones answered and which did not
- **On-disk response cache** — a URL fetched once is never fetched again until its TTL expires. The dominant access pattern for reference material is "read once, re-read many"
- **Local index** — full-text search over crawled content via Tantivy, opened once per process
- **MCP 2026-07-28** — stateless core, with the 2025-11-25 handshake kept for legacy clients
- **Single binary** — one Rust executable, minimal resource footprint

## Quick Start

**Prerequisites:** [devenv](https://devenv.sh/) and Rust 1.98+.

```bash
# 1. Start SearXNG (Nix-packaged, no container runtime involved)
devenv up

# 2. Build
cargo build --release -p bose-search

# 3. Register with your host
#    Claude Code auto-discovers .mcp.json in this directory.
#    Otherwise, point your MCP client at target/release/bose-search.
```

`devenv` is a hard dependency for *developing and running* bose-search — it owns the SearXNG process and the whole toolchain. The MCP server itself is still a plain binary: registering `devenv shell -- bose-search` with a host would add a Nix evaluation to every agent session, so the built binary is what you point at.

## Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `web_search` | Query the web through SearXNG; reports which engines answered | `query`, `num_results`, `category`, `language`, `time_range` |
| `local_search` | Query the local knowledge index | `query`, `num_results` |
| `read_url` | Fetch a URL to clean text (charset-detected, entity-decoded, 50KB cap) | `url` |
| `crawl_knowledge` | Search → fetch → extract → index, reporting cache hits and per-URL failures | `query`, `depth`, `source_filter` |
| `scavenger_search` | Surface results mainstream engines systematically miss | `query`, `harvest_size`, `num_results`, `category`, `language` |
| `recall` | Read a stored response back with its crawl timestamp, no network | `url` |
| `health_check` | SearXNG reachability + index size + cache entry count | — |

## Architecture

```
crates/
├── bose-common/     Shared types, configuration, errors, algorithms
├── bose-searxng/    SearXNG HTTP client (engine health carried through)
├── bose-index/      Tantivy index + a process-lifetime Searcher handle
├── bose-net/        Shared HTTP client, DOM extraction, on-disk response cache
├── bose-crawler/    Fetcher with PID-controlled backpressure
└── bose-search/     MCP server entry point (rmcp 3.5)
```

Every outbound fetch goes through `bose-net`, so there is exactly one connection pool, one extractor, and one cache. `SearcherHandle` opens the Tantivy index at most once per process.

### Configuration

All settings come from environment variables.

| Variable | Default | Description |
|----------|---------|-------------|
| `SEARXNG_URL` | `http://127.0.0.1:8080` | SearXNG endpoint |
| `DEFAULT_NUM_RESULTS` | `10` | Results per query |
| `REQUEST_TIMEOUT_SECS` | `30` | HTTP timeout |
| `BOSE_INDEX_PATH` | `$XDG_DATA_HOME/bose-search/index` | Index directory |
| `BOSE_CACHE_PATH` | `$XDG_CACHE_HOME/bose-search/responses` | Response cache directory |
| `BOSE_CACHE_TTL_SECS` | `604800` (7 days) | Cache freshness; `0` bypasses the cache entirely |

## Protocol support

`devenv shell -- just probe` prints both eras side by side:

```
### LEGACY ERA (initialize handshake)
  initialize  -> protocolVersion=2025-11-25
  tools/list  -> resultType=None n=7

### MODERN ERA (2026-07-28, stateless)
  discover    -> ['2024-11-05', …, '2026-07-28']
                resultType=complete ttlMs=0 cacheScope=private
  tools/list  -> resultType=complete n=7
```

A 2026-07-28 server cannot name the new revision from `initialize`, because that revision has no handshake at all — it answers `2025-11-25` and advertises the full range through `server/discover`. Legacy clients therefore keep working unchanged.

## SearXNG

`devenv up` runs `pkgs.searxng`, reading `deploy/settings.yml` — a plain file you can read and diff, rather than a container image. **Engines are fetched directly by default.** The previous configuration routed every engine through a Tor SOCKS proxy, which meant every query paid three extra hops and then failed once that proxy stopped; anonymity is now opt-in per engine (`proxies:`) rather than global.

Which engines actually answer depends on your IP: shared and datacenter addresses are throttled by Google, DuckDuckGo and Brave. The tool output names the ones that answered and the ones that did not, so an empty result set is interpretable instead of merely empty.

## Algorithms

- **RRF Fusion** (`bose-common/fusion.rs`) — Reciprocal Rank Fusion merges ranked lists from multiple engines
- **Position-aware Blending** (`bose-common/blending.rs`) — Interleaves results while preserving positional relevance
- **PID Backpressure** (`bose-crawler/pid.rs`) — Concurrency controller driven by latency and CPU
- **SHA-256 Deduplication** (`bose-common/dedup.rs`) — Content-hash dedup across engines
- **Document Chunking** (`bose-common/chunker.rs`) — Splitting with configurable overlap
- **Scavenger Algorithm** (`bose-common/scavenger.rs`) — Information-theoretic surprise scoring `S(r) = −log₂(freq(r)/|E|)` with greedy submodular selection `(1−1/e)` approximation

## Roadmap

- [x] ScanXNG meta-search integration
- [x] MCP 2026-07-28 stateless core (dual-era with legacy clients)
- [x] Persistent on-disk response cache (`recall`, `BOSE_CACHE_TTL_SECS`)
- [x] Shared HTTP/extraction layer replacing three per-call client constructions
- [x] UTF-8-safe truncation (the old byte-slice truncated mid-code-point)
- [ ] Direct-engine tier with browser TLS fingerprinting (`wreq`), for targets that block SearXNG
- [ ] `bose-tor`: in-process arti SOCKS proxy, opt-in per request
- [ ] Common Crawl CDX tier for already-archived pages

## Licence

Licensed under either [Apache Licence, Version 2.0](LICENSE-APACHE) or [MIT Licence](LICENSE-MIT) at your option.

SearXNG, run as a separate process by `devenv`, is licensed AGPL-3.0-or-later. It is not linked into this project.

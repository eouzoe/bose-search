# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-16

### Added
- MCP server with `web_search` and `health_check` tools
- SearXNG integration supporting 247 search engines (Google, Brave, DuckDuckGo, Startpage)
- Tantivy-based local document indexing with full-text search
- RRF (Reciprocal Rank Fusion) result merging algorithm
- Position-aware blending for hybrid search (web + local)
- Content deduplication via SHA-256 hashing
- Document chunking (800 tokens, 15% overlap)
- HTML content extraction with tag filtering
- PID-controlled crawl rate limiting based on CPU usage
- Podman deployment configuration for SearXNG
- Dual licensing (MIT OR Apache-2.0)

### Technical Details
- 5 crates: bose-common, bose-searxng, bose-search, bose-index, bose-crawler
- 25 Rust source files, ~4,474 lines of code
- 128 tests across 5 crates
- Zero clippy warnings
- Rust 2024 edition

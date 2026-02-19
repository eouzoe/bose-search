# Architecture

> Work in progress. This document will cover the five-layer architecture, crate responsibilities, data flow, and algorithm design decisions.

## Layers

```
Layer 5: MCP Protocol     — rmcp 0.14, tool dispatch, structured responses
Layer 4: Smart Routing    — query classification, backend selection, result fusion
Layer 3: Search Backends  — SearXNG (247 engines), Tantivy local index
Layer 2: Crawler          — monoio (io_uring), PID backpressure, HTML extraction
Layer 1: Storage          — rkyv zero-copy, SIMD BP128 compression
```

## Crates

| Crate | Responsibility |
|-------|---------------|
| `bose-search` | MCP server entry point, tool handlers, router |
| `bose-searxng` | SearXNG HTTP client, result normalisation |
| `bose-common` | Shared types, algorithms (RRF, scavenger, PID, dedup) |
| `bose-index` | Tantivy wrapper, document ingestion, full-text search |
| `bose-crawler` | HTTP fetcher, rate limiting, content extraction |

## Key Algorithms

See inline rustdoc in each module for mathematical foundations and references.

---

*[English](../README.md) · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md)*

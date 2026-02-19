#!/usr/bin/env bash
# Bose Search - Development Environment Initialization Script
# This script sets up the development environment for the project

set -euo pipefail

echo "🚀 Initializing Bose Search development environment..."

# 1. Start SearXNG container
echo "📦 Starting SearXNG container..."
if podman ps -a | grep -q bose-searxng; then
    if ! podman ps | grep -q bose-searxng; then
        podman start bose-searxng
        echo "✅ SearXNG container started"
    else
        echo "✅ SearXNG container already running"
    fi
else
    echo "⚠️  SearXNG container not found. Please run:"
    echo "   cd deploy && podman-compose up -d"
fi

# 2. Check Rust toolchain
echo "🦀 Checking Rust toolchain..."
if ! command -v rustc &> /dev/null; then
    echo "❌ Rust not found. Please install from https://rustup.rs/"
    exit 1
fi
echo "✅ Rust $(rustc --version)"

# 3. Check cargo tools
echo "🔧 Checking development tools..."
MISSING_TOOLS=()

for tool in cargo-audit cargo-deny cargo-machete cargo-outdated typos taplo gitleaks; do
    if ! command -v "$tool" &> /dev/null; then
        MISSING_TOOLS+=("$tool")
    fi
done

if [ ${#MISSING_TOOLS[@]} -gt 0 ]; then
    echo "⚠️  Missing tools: ${MISSING_TOOLS[*]}"
    echo "   Install with: cargo install ${MISSING_TOOLS[*]}"
else
    echo "✅ All development tools installed"
fi

# 4. Check environment variables
echo "🔑 Checking environment variables..."
if [ -z "${SEARXNG_URL:-}" ]; then
    echo "⚠️  SEARXNG_URL not set, using default: http://localhost:8080"
    export SEARXNG_URL="http://localhost:8080"
fi
echo "✅ SEARXNG_URL=$SEARXNG_URL"

if [ -z "${EXA_API_KEY:-}" ]; then
    echo "⚠️  EXA_API_KEY not set (optional for Exa MCP)"
fi

# 5. Run tests
echo "🧪 Running tests..."
if cargo test --quiet; then
    echo "✅ All tests passed"
else
    echo "❌ Tests failed"
    exit 1
fi

# 6. Run security scans
echo "🔒 Running security scans..."
cargo audit --quiet || echo "⚠️  cargo-audit found issues"
cargo deny check || echo "⚠️  cargo-deny found issues"

echo ""
echo "✨ Environment initialized successfully!"
echo ""
echo "Next steps:"
echo "  - Run tests: cargo test"
echo "  - Run benchmarks: cargo bench"
echo "  - Build release: cargo build --release"
echo "  - Run MCP server: ./target/release/bose-mcp"

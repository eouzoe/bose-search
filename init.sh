#!/usr/bin/env bash
# Bose Search - Development Environment Initialization Script
# This script sets up the development environment for the project

set -euo pipefail

echo "🚀 Initializing Bose Search development environment..."

# 1. Start SearXNG + Tor via podman-compose
echo "Starting SearXNG + Tor containers..."
if podman ps | grep -q bose-searxng && podman ps | grep -q bose-tor; then
    echo "SearXNG + Tor containers already running"
else
    (cd deploy && podman-compose up -d)
    echo "Containers started"
fi

# 2. Check Rust toolchain
echo "Checking Rust toolchain..."
if ! command -v rustc &> /dev/null; then
    echo "Rust not found. Please install from https://rustup.rs/"
    exit 1
fi
echo "Rust $(rustc --version)"

# 3. Check cargo tools
echo "Checking development tools..."
MISSING_TOOLS=()

for tool in cargo-audit cargo-deny cargo-machete cargo-outdated typos taplo gitleaks; do
    if ! command -v "$tool" &> /dev/null; then
        MISSING_TOOLS+=("$tool")
    fi
done

if [ ${#MISSING_TOOLS[@]} -gt 0 ]; then
    echo "Missing tools: ${MISSING_TOOLS[*]}"
    echo "  Install with: cargo install ${MISSING_TOOLS[*]}"
else
    echo "All development tools installed"
fi

# 4. Run tests
echo "Running tests..."
if cargo test --quiet; then
    echo "All tests passed"
else
    echo "Tests failed"
    exit 1
fi

# 5. Run security scans
echo "Running security scans..."
cargo audit --quiet || echo "cargo-audit found issues"
cargo deny check || echo "cargo-deny found issues"

echo ""
echo "Environment initialized successfully!"
echo ""
echo "Next steps:"
echo "  - Build MCP binary: cargo build --release -p bose-mcp"
echo "  - Verify MCP server: target/release/bose-mcp"

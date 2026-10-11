# bose-search development tasks.
#
# `devenv shell` first, or `just <task>` inside the devenv environment.

# Start SearXNG (Nix-packaged, no container). Then leave it running.
up:
    devenv up

# Build the release binary.
build:
    cargo build --release -p bose-search

# Fast test loop.
test *ARGS:
    cargo test --workspace {{ARGS}}

# Lints. `cargo clippy -- -D warnings` is the project standard.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

# Format + lint + test in one go.
check: fmt lint test

# Show the MCP tool surface as JSON, to verify the declared protocol version
# and tool list without a host client.
probe:
    ./scripts/probe.sh

# Register the built binary with the current project's MCP config.
register: build
    @mkdir -p .opencode
    @if [ ! -f .opencode/opencode.json ]; then \
        echo '{"$schema":"https://opencode.ai/config.json"}' > .opencode/opencode.json; \
    fi
    @echo "registered: $(pwd)/target/release/bose-search"

# Stop the process manager.
down:
    devenv processes down

# Full secret sweep across the entire history. Slow (~4 min) and deliberately
# NOT in the pre-push gate: that one is incremental (`gitleaks protect
# --staged`). Run this on a release, or after rotating a credential.
security-full:
    gitleaks detect --no-git --verbose

# Security gate as CI would run it: everything, not just the staged diff.
ci-security: security-full
    cargo audit
    cargo deny check

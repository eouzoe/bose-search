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

# Call a bose-search tool directly over stdio, bypassing opencode entirely.
# Hard timeout, so it can never hang. This is the safe path when the
# opencode-managed channel is wedged (see mcp-watch).
#
#   just mcp-call health_check
#   just mcp-call web_search '{"query":"test","num_results":5}' 25
mcp-call TOOL="health_check" ARGS='{}' TIMEOUT="20":
    @./scripts/mcp-call.sh "{{TOOL}}" '{{ARGS}}' "{{TIMEOUT}}"

# Reconnect the bose-search MCP server to the opencode service when its stdio
# connection breaks. Normally mcp-watch does this automatically; this is the
# manual escape hatch.
#
# `location` is a deepObject query parameter — passing it as a plain string
# yields "Expected object | undefined at [\"location\"]".
mcp-reconnect:
    @URL=$$(python3 -c "import json;print(json.load(open('$HOME/.local/state/opencode/service.json'))['url'])"); \
     PW=$$(python3 -c "import json;print(json.load(open('$HOME/.local/state/opencode/service.json'))['password'])"); \
     LOC=$$PWD; \
     echo "disconnecting..."; \
     curl -sS -X POST -u "opencode:$$PW" "$$URL/api/experimental/mcp/bose-search/disconnect" \
       --get --data-urlencode "location[directory]=$$LOC" -o /dev/null -w "  http=%{http_code}\n"; \
     sleep 2; \
     echo "connecting..."; \
     curl -sS -X POST -u "opencode:$$PW" "$$URL/api/experimental/mcp/bose-search/connect" \
       --get --data-urlencode "location[directory]=$$LOC" -o /dev/null -w "  http=%{http_code}\n"; \
     sleep 5; \
     echo "final status:"; \
     curl -sS -u "opencode:$$PW" --get "$$URL/api/mcp" \
       --data-urlencode "location[directory]=$$LOC" 2>/dev/null | \
       python3 -c "import sys,json; [print('  ',s['name'],'->',s.get('status',{}).get('status')) for s in json.load(sys.stdin).get('data',[]) if s['name']=='bose-search']"

# Watch the bose-search MCP status and force-reconnect the moment it breaks.
#
# opencode holds one stdio connection per server for the whole session, with
# no request timeout and no respawn: once the server process exits or the pipe
# wedges, every tool call hangs forever while the UI still says "connected".
# This watchdog polls the service's own view and reconnects proactively.
#
#   just mcp-watch
#   just mcp-watch 5        # poll every 5 seconds
mcp-watch INTERVAL="3":
    @echo "watching bose-search (location=$$PWD) — Ctrl-C to stop"
    @./scripts/mcp-watchdog.sh "$$PWD" "{{INTERVAL}}" /tmp/bose-watchdog.log

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

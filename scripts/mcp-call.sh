#!/bin/bash
# Call a bose-search MCP tool directly over stdio, with a hard timeout.
#
#   ./scripts/mcp-call.sh <tool> [json-args] [timeout-secs]
#
# This bypasses opencode's MCP client entirely. opencode holds one stdio
# connection per server for the whole session, has no request timeout and
# no respawn — so when that channel wedges, every tool call hangs forever
# while the UI still says "connected". Calling the binary directly over
# stdio can never hang: `timeout` kills the process if it does not answer
# within the budget.
#
# Data flows through environment variables and the program through a
# heredoc, so arbitrary JSON arguments are never mangled by shell quoting.
#
# Examples:
#   ./scripts/mcp-call.sh health_check
#   ./scripts/mcp-call.sh web_search '{"query":"plague Russia 2026","num_results":5}' 25
#   ./scripts/mcp-call.sh recall '{"url":"https://example.com/"}' 15
#
# Exit codes: 0 success, 124 timeout, 1 other error.
set -uo pipefail

TOOL="${1:-health_check}"
ARGS="${2:-\{\}}"
TIMEOUT_SECS="${3:-20}"
BIN="${BOSE_BIN:-$HOME/.cargo/bin/bose-search}"

export BOSE_CALL_TOOL="$TOOL"
export BOSE_CALL_ARGS="$ARGS"

# Build the JSON-RPC request.
REQ=$(python3 - <<'PY'
import json, os
req = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": os.environ["BOSE_CALL_TOOL"],
        "arguments": json.loads(os.environ["BOSE_CALL_ARGS"]),
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "mcp-call", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {"tools": {"listChanged": False}},
        },
    },
}
print(json.dumps(req))
PY
) || { echo "bad arguments: $ARGS" >&2; exit 1; }

t0=$(date +%s.%N)
# Send the request and close stdin. The server answers, then sees
# EOF and exits; `head -1` takes the first response line and the
# pipeline returns as soon as the server is done. `timeout` is a
# hard safety net so a wedged server is always killed within budget.
OUT=$( printf '%s\n' "$REQ" | timeout "$TIMEOUT_SECS" "$BIN" 2>/dev/null | head -1 )
t1=$(date +%s.%N)

if [ -z "$OUT" ]; then
    echo "TIMEOUT: no response from $BIN within ${TIMEOUT_SECS}s (tool=$TOOL)" >&2
    exit 124
fi

export BOSE_CALL_OUT="$OUT"
export BOSE_CALL_ELAPSED=$(python3 -c "print(f'{$t1-$t0:.2f}')")

python3 - <<'PY'
import json, os, sys
d = json.loads(os.environ["BOSE_CALL_OUT"])
elapsed = os.environ["BOSE_CALL_ELAPSED"]
if "error" in d:
    e = d["error"]
    print(f"ERROR {e.get('code')}: {e.get('message')}")
    sys.exit(1)
r = d.get("result", {})
text = (r.get("content") or [{}])[0].get("text", "")
print(f"[{elapsed}s] resultType={r.get('resultType')} isError={r.get('isError')}")
print(text)
PY

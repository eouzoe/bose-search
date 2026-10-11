#!/bin/bash
# Probe the MCP server on both protocol eras and print a compact summary.
#
#   ./scripts/probe.sh [path-to-binary]
#
# Two passes:
#   legacy  — the `initialize` handshake. A 2026-07-28 server cannot name the
#             new revision here (that revision has no handshake at all), so it
#             answers `2025-11-25` — LATEST_WITH_INITIALIZE.
#   modern  — no handshake, `server/discover` then self-describing requests
#             carrying protocolVersion in `_meta`. Server results must carry
#             `resultType`.
set -uo pipefail

BIN="${1:-target/release/bose-search}"

LEGACY_META='"capabilities":{},"clientInfo":{"name":"probe","version":"1"}'
MODERN_META='"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"probe","version":"1"},"io.modelcontextprotocol/clientCapabilities":{"tools":{"listChanged":false}}'

# Collect to a file first: a heredoc inside the parser would otherwise consume
# the pipe, and the JSON would be read by the wrong process.
RAW=$(mktemp)
trap 'rm -f "$RAW"' EXIT

hr() { printf '%s\n' "------------------------------------------------------------"; }

summarise() {
    python3 - "$RAW" <<'PY'
import json, sys
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    try:
        d = json.loads(line)
    except json.JSONDecodeError:
        print(f"  [non-JSON] {line[:120]}")
        continue
    if "error" in d:
        e = d["error"]
        print(f"  ERROR {e.get('code')}: {e.get('message')}")
        continue
    r = d.get("result", {})
    if "protocolVersion" in r:
        print(f"  initialize  -> protocolVersion={r['protocolVersion']}")
    elif "supportedVersions" in r:
        print(f"  discover    -> {r['supportedVersions']}")
        print(f"                resultType={r.get('resultType')} ttlMs={r.get('ttlMs')} cacheScope={r.get('cacheScope')}")
    elif "tools" in r:
        names = sorted(t['name'] for t in r.get('tools', []))
        print(f"  tools/list  -> resultType={r.get('resultType')} n={len(names)}")
        print(f"                {', '.join(names)}")
    elif "content" in r:
        first = (r.get("content") or [{}])[0].get("text", "")
        print(f"  tools/call  -> resultType={r.get('resultType')} isError={r.get('isError')}")
        print(f"                {first[:100]!r}")
    else:
        print(f"  result      -> {sorted(r.keys())}")
PY
}

echo "### LEGACY ERA (initialize handshake)"
hr
: > "$RAW"
{
    printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{%s,"protocolVersion":"2026-07-28"}}\n' "$LEGACY_META"
    printf '{"jsonrpc":"2.0","method":"notifications/initialized"}\n'
    printf '{"jsonrpc":"2.0","id":2,"method":"tools/list"}\n'
    printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"health_check","arguments":{}}}\n'
    sleep 10
} | timeout 25 "$BIN" 2>/dev/null >> "$RAW"
summarise

echo
echo "### MODERN ERA (2026-07-28, stateless)"
hr
: > "$RAW"
{
    printf '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{%s}}}\n' "$MODERN_META"
    printf '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{%s}}}\n' "$MODERN_META"
    printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"health_check","arguments":{},"_meta":{%s}}}\n' "$MODERN_META"
    sleep 10
} | timeout 25 "$BIN" 2>/dev/null >> "$RAW"
summarise

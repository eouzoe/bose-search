#!/bin/bash
# Watchdog for the bose-search MCP server as seen by the opencode service.
#
#   ./scripts/mcp-watchdog.sh [location-directory] [interval-secs] [logfile]
#
# Why this exists. opencode's MCP client holds a single stdio connection
# per server for the lifetime of the session. It has no request timeout
# and no respawn. When that connection breaks — the server process exits,
# is killed, or the pipe wedges — the service still reports "connected"
# for a while and then "failed", and every subsequent tool call hangs
# forever with no error surfaced. That is the "卡死" mode.
#
# This watchdog polls the service's own view of the server (the same
# view that gates tool calls) and forces a reconnect the moment the
# status is anything but "connected", so the channel is live before the
# next call is made.
#
# It reads the service URL and password from
# ~/.local/state/opencode/service.json, so it works without any
# configuration. Run it in the background:
#
#   ./scripts/mcp-watchdog.sh &
#
# Reconnect flow (the same two calls that fix a wedged channel by hand):
#   POST /api/experimental/mcp/bose-search/disconnect?location[directory]=...
#   POST /api/experimental/mcp/bose-search/connect?location[directory]=...
#
# Note: `location` is a deepObject query parameter, not a plain string —
# feeding it as a string yields "Expected object | undefined at [\"location\"]".
set -uo pipefail

LOC="${1:-$PWD}"
INTERVAL="${2:-5}"
LOG="${3:-/tmp/bose-watchdog.log}"

SERVICE_JSON="$HOME/.local/state/opencode/service.json"

log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" >> "$LOG"; }

if [ ! -f "$SERVICE_JSON" ]; then
    echo "opencode service not running: $SERVICE_JSON missing" >&2
    exit 1
fi

read -r URL PASSWORD < <(python3 -c "
import json
d = json.load(open('$SERVICE_JSON'))
print(d['url'], d['password'])
")

AUTH="opencode:$PASSWORD"

# Report the server's status exactly as the service sees it.
status() {
    curl -sS -u "$AUTH" --get "$URL/api/mcp" \
        --data-urlencode "location[directory]=$LOC" 2>/dev/null \
        | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    print('parse-error')
    sys.exit()
for s in d.get('data', []):
    if s['name'] == 'bose-search':
        print(s.get('status', {}).get('status', 'unknown'))
        sys.exit()
print('absent')
"
}

reconnect() {
    log "forcing reconnect (location=$LOC)"
    curl -sS -X POST -u "$AUTH" \
        "$URL/api/experimental/mcp/bose-search/disconnect" \
        --get --data-urlencode "location[directory]=$LOC" \
        -o /dev/null 2>/dev/null
    sleep 2
    curl -sS -X POST -u "$AUTH" \
        "$URL/api/experimental/mcp/bose-search/connect" \
        --get --data-urlencode "location[directory]=$LOC" \
        -o /dev/null 2>/dev/null
    sleep 4
}

log "watchdog started: location=$LOC interval=${INTERVAL}s log=$LOG"
while true; do
    st=$(status)
    case "$st" in
        connected)
            ;;
        *)
            log "status=$st — not connected, reconnecting"
            reconnect
            st2=$(status)
            log "after reconnect: status=$st2"
            ;;
    esac
    sleep "$INTERVAL"
done

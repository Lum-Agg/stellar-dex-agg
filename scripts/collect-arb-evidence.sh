#!/usr/bin/env bash
# Summarize arb bot SUCCESS txs for Tranche 2 operator evidence.
#
# Usage (on server):
#   ./scripts/collect-arb-evidence.sh
#   ./scripts/collect-arb-evidence.sh --since "2026-07-13"
#
# Remote:
#   ssh root@178.63.81.216 'bash -s' < scripts/collect-arb-evidence.sh
set -euo pipefail

SINCE="${1:-2026-07-13 00:00:00}"
if [[ "${1:-}" == "--since" ]]; then
  SINCE="${2:?usage: --since \"YYYY-MM-DD HH:MM:SS\"}"
fi

UNIT="${ARB_UNIT:-lumagg-arb}"

echo "# Arb evidence — journalctl -u $UNIT --since \"$SINCE\""
echo

COUNT=$(journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null | grep -c "arb tx SUCCESS" || true)
echo "SUCCESS count: $COUNT"
echo

echo "## Recent SUCCESS hashes"
journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null \
  | grep "arb tx SUCCESS" \
  | sed -E 's/.*hash=([a-f0-9]{64}).*/\1/' \
  | tail -20 \
  | while read -r h; do
    echo "- https://stellar.expert/explorer/public/tx/$h"
  done

echo
echo "## Sample log lines"
journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null \
  | grep "arb tx SUCCESS" | tail -5

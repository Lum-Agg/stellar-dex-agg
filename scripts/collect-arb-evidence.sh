#!/usr/bin/env bash
# Summarize arb bot SUCCESS txs for Tranche 2 operator evidence.
#
# Usage (on server):
#   ./scripts/collect-arb-evidence.sh
#   ./scripts/collect-arb-evidence.sh --since "2026-07-13 00:00:00"
#   ./scripts/collect-arb-evidence.sh --output docs/arb-evidence-snapshot.md
#
# Remote:
#   ssh root@178.63.81.216 'bash -s' < scripts/collect-arb-evidence.sh
set -euo pipefail

SINCE="2026-07-13 00:00:00"
OUTPUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --since)
      SINCE="${2:?usage: --since \"YYYY-MM-DD HH:MM:SS\"}"
      shift 2
      ;;
    --output|-o)
      OUTPUT="${2:?usage: --output path.md}"
      shift 2
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

UNIT="${ARB_UNIT:-lumagg-arb}"
GENERATED="$(date -u +%Y-%m-%dT%H:%MZ)"

emit() {
  if [[ -n "$OUTPUT" ]]; then
    tee "$OUTPUT"
  else
    cat
  fi
}

{
  echo "# Arb operator evidence snapshot"
  echo
  echo "Generated: $GENERATED · unit: \`$UNIT\` · since: \`$SINCE\`"
  echo

  COUNT=$(journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null | grep -c "arb tx SUCCESS" || true)
  echo "**SUCCESS count:** $COUNT"
  echo

  echo "## Recent SUCCESS hashes"
  echo
  journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null \
    | grep "arb tx SUCCESS" \
    | sed -E 's/.*hash=([a-f0-9]{64}).*/\1/' \
    | tail -20 \
    | while read -r h; do
      echo "- https://stellar.expert/explorer/public/tx/$h"
    done

  echo
  echo "## Sample log lines"
  echo '```'
  journalctl -u "$UNIT" --since "$SINCE" --no-pager 2>/dev/null \
    | grep "arb tx SUCCESS" | tail -5
  echo '```'
} | emit

if [[ -n "$OUTPUT" ]]; then
  echo "Wrote $OUTPUT" >&2
fi

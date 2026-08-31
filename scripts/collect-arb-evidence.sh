#!/usr/bin/env bash
# Summarize arb bot SUCCESS txs for Tranche 2 operator evidence.
#
# Usage (on server):
#   ./scripts/collect-arb-evidence.sh
#   ./scripts/collect-arb-evidence.sh --since "2026-07-13 00:00:00"
#   ./scripts/collect-arb-evidence.sh --output docs/arb-evidence-snapshot.md
#
# Remote:
#   ssh root@88.198.16.144 'bash -s' < scripts/collect-arb-evidence.sh
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
STATS_URL="${ARB_STATS_URL:-https://api.lumagg.xyz/api/v1/arbitrage/stats}"
GENERATED="$(date -u +%Y-%m-%dT%H:%MZ)"

to_unix() {
  if date -d "$1" +%s 2>/dev/null; then
    return
  fi
  date -j -f "%Y-%m-%d %H:%M:%S" "$1" +%s
}

START_UNIX="$(to_unix "$SINCE")"
END_UNIX="$(date -u +%s)"

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

  STATS_JSON=$(curl --fail --silent --show-error --get "$STATS_URL" \
    --data-urlencode "granularity=day" \
    --data-urlencode "start=$START_UNIX" \
    --data-urlencode "end=$END_UNIX")
  SUCCESS_COUNT=$(jq -r '[.data.buckets[]?.success_count // 0] | add // 0' <<<"$STATS_JSON")
  FAILED_COUNT=$(jq -r '[.data.buckets[]?.failed_count // 0] | add // 0' <<<"$STATS_JSON")
  echo "**Confirmed SUCCESS count:** $SUCCESS_COUNT"
  echo "**Confirmed FAILED count:** $FAILED_COUNT"
  echo
  echo "Counts come from the analytics indexer via `/api/v1/arbitrage/stats`;"
  echo "the journal below is used only for recent hashes and operator log samples."
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
  echo
  echo "## Vault / aggregator"
  echo
  echo '| | |'
  echo '|--|--|'
  echo '| Vault | \`CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN\` |'
  echo '| Aggregator | \`CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K\` |'
  echo '| Runbook | [arb-operator.md](./arb-operator.md) |'
} | emit

if [[ -n "$OUTPUT" ]]; then
  echo "Wrote $OUTPUT" >&2
fi

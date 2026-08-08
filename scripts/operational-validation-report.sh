#!/usr/bin/env bash
set -euo pipefail

# Build a grant/operations snapshot from the public API. No credentials are read.
API_URL="${API_URL:-https://api.lumagg.xyz}"
OUTPUT="${OUTPUT:--}"

usage() {
  cat <<'EOF'
Usage: scripts/operational-validation-report.sh [options]

Options:
  --api URL       API base URL (default: https://api.lumagg.xyz)
  --output FILE   Write Markdown report and FILE.json; default: stdout
  -h, --help      Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --api) API_URL="${2:?missing URL after --api}"; shift 2 ;;
    --output) OUTPUT="${2:?missing FILE after --output}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

API_URL="${API_URL%/}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

curl -fsSL "$API_URL/api/v1/health" >"$tmp_dir/health.json"
curl -fsSL "$API_URL/api/v1/ready" >"$tmp_dir/ready.json"
curl -fsSL "$API_URL/api/v1/stats" >"$tmp_dir/stats.json"

generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
health_json="$(cat "$tmp_dir/health.json")"
ready_json="$(cat "$tmp_dir/ready.json")"
report_json="$(jq -c \
  --arg api "$API_URL" \
  --arg generated "$generated_at" \
  --argjson health "$health_json" \
  --argjson ready "$ready_json" \
  '
    .data as $data |
    ($data.daily // []) as $days |
    {
      generated_at: $generated,
      api_url: $api,
      health: {ok: ($health.status == "ok"), status: ($health.status // "unknown")},
      ready: {ok: ($ready.ready == true), status: ($ready.status // "unknown"), tokens: ($ready.tokens // null), pools: ($ready.pools // null)},
      indexer: {
        cursor_ledger: $data.cursor_ledger,
        invocation_count: $data.invocation_count,
        days_indexed: ($days | length),
        oldest_day: ($days[0].day // null),
        newest_day: ($days[-1].day // null),
        transaction_count: ($days | map(.tx_count) | add // 0),
        unique_users_sum: ($days | map(.unique_users) | add // 0),
        routed_leg_count: ($days | map(.routed_leg_count // 0) | add // 0),
        round_trip_count: ($days | map(.round_trip_count // 0) | add // 0),
        total_amount_in_usd: ($days | map(.total_amount_in_usd // 0) | add // 0),
        routed_volume_usd: ($days | map(.total_routed_dex_volume_usd // 0) | add // 0),
        gross_surplus_usd: ($days | map(.round_trip_gross_surplus_usd // 0) | add // 0),
        by_function: ($days | map(.by_function // {}) | add),
        by_dex: ($days | map(.by_dex // {}) | add)
      },
      acceptance: {
        thirty_day_target_met: (($days | length) >= 30),
        public_health_ok: ($health.status == "ok"),
        public_ready_ok: ($ready.ready == true)
      }
    }
  ' "$tmp_dir/stats.json")"

echo "$report_json" | jq -e 'type == "object" and .indexer.days_indexed >= 0' >/dev/null

markdown="$(echo "$report_json" | jq -r '
  "# LumAgg Operational Validation Report\n\n" +
  "Generated: `" + .generated_at + "`  \n" +
  "API: [" + .api_url + "](" + .api_url + ")\n\n" +
  "## Public service\n\n" +
  "| Check | Result |\n| --- | --- |\n" +
  "| Health endpoint | `" + (.health.ok | tostring) + "` (`" + .health.status + "`) |\n" +
  "| Ready endpoint | `" + (.ready.ok | tostring) + "` (`" + .ready.status + "`) |\n\n" +
  "## Indexed data\n\n" +
  "| Metric | Value |\n| --- | --- |\n" +
  "| Ledger cursor | `" + ((.indexer.cursor_ledger // "—") | tostring) + "` |\n" +
  "| Days indexed | `" + (.indexer.days_indexed | tostring) + "` |\n" +
  "| Coverage | `" + ((.indexer.oldest_day // "—") | tostring) + "` → `" + ((.indexer.newest_day // "—") | tostring) + "` |\n" +
  "| Aggregator invocations | `" + (.indexer.invocation_count | tostring) + "` |\n" +
  "| Successful transactions | `" + (.indexer.transaction_count | tostring) + "` |\n" +
  "| DEX legs | `" + (.indexer.routed_leg_count | tostring) + "` |\n" +
  "| Round trips | `" + (.indexer.round_trip_count | tostring) + "` |\n" +
  "| Entry notional (USD) | `$" + (.indexer.total_amount_in_usd | tostring) + "` |\n" +
  "| Routed DEX volume (USD) | `$" + (.indexer.routed_volume_usd | tostring) + "` |\n" +
  "| Gross surplus (USD) | `$" + (.indexer.gross_surplus_usd | tostring) + "` |\n\n" +
  "## Acceptance\n\n" +
  "- 30-day data target: `" + (.acceptance.thirty_day_target_met | tostring) + "`\n" +
  "- Public health: `" + (.acceptance.public_health_ok | tostring) + "`\n" +
  "- Public readiness: `" + (.acceptance.public_ready_ok | tostring) + "`\n"
')"

if [[ "$OUTPUT" == "-" ]]; then
  printf '%s\n' "$markdown"
else
  mkdir -p "$(dirname "$OUTPUT")"
  printf '%s\n' "$markdown" >"$OUTPUT"
  printf '%s\n' "$report_json" | jq . >"$OUTPUT.json"
  echo "Wrote $OUTPUT and $OUTPUT.json" >&2
fi

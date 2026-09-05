#!/usr/bin/env bash
# Protocol 27 prep: quick API regression (health, quote, optional stats).
#
# Usage:
#   API=https://api.lumagg.xyz/limit-testnet ./scripts/p27-testnet-smoke.sh
#   API=http://127.0.0.1:3200 ./scripts/p27-testnet-smoke.sh
set -euo pipefail

API="${API:-https://api.lumagg.xyz/limit-testnet}"
# Current Soroswap testnet assets used by the deployed Limit/DCA stack.
XLM="${XLM:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}"
USDC="${USDC:-CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F}"
AMOUNT="${AMOUNT_IN:-10000000}"

fail=0
check_json() {
  local name="$1"
  local filter="$2"
  shift
  shift
  local response
  if response=$("$@" 2>/dev/null) && jq -e "$filter" >/dev/null <<<"$response"; then
    echo "PASS  $name"
  else
    echo "FAIL  $name" >&2
    fail=1
  fi
}

echo "=== P27 smoke API=$API ==="

check_json "health" '.status == "ok"' curl -sf "$API/api/v1/health"

check_json "quote" '.success == true and (.data.expected_output | type == "string")' curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=$AMOUNT" \
  --data-urlencode "slippage=0.5"

check_json "prefer_soroban quote" '.success == true and (.data.expected_output | type == "string")' curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=$AMOUNT" \
  --data-urlencode "prefer_soroban=1"

if stats="$(curl -sf "$API/api/v1/stats" 2>/dev/null)" && jq -e '.success == true' >/dev/null <<<"$stats"; then
  echo "PASS  stats"
else
  echo "SKIP  stats (indexer DB not mounted or empty)"
fi

echo
if [[ $fail -eq 0 ]]; then
  echo "All required checks passed."
else
  echo "Some checks failed." >&2
  exit 1
fi

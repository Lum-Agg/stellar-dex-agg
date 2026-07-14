#!/usr/bin/env bash
# Protocol 27 prep: quick API regression (health, quote, optional stats).
#
# Usage:
#   API=https://api.lumagg.xyz ./scripts/p27-testnet-smoke.sh
#   API=http://127.0.0.1:3100 ./scripts/p27-testnet-smoke.sh
set -euo pipefail

API="${API:-https://api.lumagg.xyz}"
XLM="${XLM:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}"
USDC="${USDC:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}"
AMOUNT="${AMOUNT_IN:-10000000}"

fail=0
check() {
  local name="$1"
  shift
  if "$@"; then
    echo "PASS  $name"
  else
    echo "FAIL  $name" >&2
    fail=1
  fi
}

echo "=== P27 smoke API=$API ==="

check "health" curl -sf "$API/api/v1/health" | grep -q '"status":"ok"'

check "quote" curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=$AMOUNT" \
  --data-urlencode "slippage=0.5" | grep -q '"success":true'

check "prefer_soroban quote" curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=$AMOUNT" \
  --data-urlencode "prefer_soroban=1" | grep -q '"success":true'

if curl -sf "$API/api/v1/stats" | grep -q '"success":true'; then
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

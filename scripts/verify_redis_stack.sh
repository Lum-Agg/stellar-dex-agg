#!/usr/bin/env bash
# Verify Redis snapshot + pool-state stack (run on server or locally).
# Usage:
#   ./scripts/verify_redis_stack.sh
#   API_BASE=http://127.0.0.1:3100 REDIS_URL='redis://:pass@127.0.0.1:6379/' ./scripts/verify_redis_stack.sh
set -euo pipefail

API_BASE="${API_BASE:-http://127.0.0.1:3100}"
REDIS_URL="${REDIS_URL:-redis://127.0.0.1:6379/}"
XLM="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
MIN_XYK_KEYS="${MIN_XYK_KEYS:-50}"
POOL_WAIT_SECS="${POOL_WAIT_SECS:-300}"

echo "=== systemd ==="
for u in lumagg-worker lumagg-api@3100; do
  if systemctl list-units --all "$u" &>/dev/null; then
    systemctl is-active "$u" && echo "$u: active" || { echo "$u: NOT active"; exit 1; }
  else
    echo "skip $u (unit not installed)"
  fi
done

echo ""
echo "=== API health ==="
curl -sf "${API_BASE}/api/v1/health" | head -c 400
echo ""

REDIS_ARGS=()
if command -v redis-cli >/dev/null; then
  REDIS_CLI_HOST=$(echo "$REDIS_URL" | sed -n 's|redis://[^@]*@\([^:/]*\).*|\1|p')
  REDIS_CLI_PORT=$(echo "$REDIS_URL" | sed -n 's|redis://[^@]*@[^:]*:\([0-9]*\).*|\1|p')
  REDIS_CLI_PORT="${REDIS_CLI_PORT:-6379}"
  REDIS_CLI_AUTH=$(echo "$REDIS_URL" | sed -n 's|redis://:\([^@]*\)@.*|\1|p')
  REDIS_ARGS=(-h "${REDIS_CLI_HOST:-127.0.0.1}" -p "$REDIS_CLI_PORT")
  if [[ -n "$REDIS_CLI_AUTH" ]]; then
    REDIS_ARGS+=(-a "$REDIS_CLI_AUTH" --no-auth-warning)
  fi
fi

echo ""
echo "=== Redis snapshot + pool keys (wait up to ${POOL_WAIT_SECS}s) ==="
XYK_COUNT=0
CLMM_COUNT=0
if command -v redis-cli >/dev/null; then
  deadline=$((SECONDS + POOL_WAIT_SECS))
  while (( SECONDS < deadline )); do
    EXISTS=$(redis-cli "${REDIS_ARGS[@]}" EXISTS lumagg:snapshot:current)
    XYK_COUNT=$(redis-cli "${REDIS_ARGS[@]}" --scan --pattern 'lumagg:pool:xyk:*' 2>/dev/null | wc -l | tr -d ' ')
    CLMM_COUNT=$(redis-cli "${REDIS_ARGS[@]}" --scan --pattern 'lumagg:pool:clmm:*' 2>/dev/null | wc -l | tr -d ' ')
    echo "  snapshot=$EXISTS xy:k=$XYK_COUNT clmm=$CLMM_COUNT"
    if [[ "$EXISTS" == "1" && "${XYK_COUNT:-0}" -ge "$MIN_XYK_KEYS" ]]; then
      break
    fi
    sleep 5
  done
  if [[ "$EXISTS" != "1" ]]; then
    echo "FAIL: lumagg:snapshot:current missing after wait"
    exit 1
  fi
  if [[ "${XYK_COUNT:-0}" -lt "$MIN_XYK_KEYS" ]]; then
    echo "FAIL: expected at least $MIN_XYK_KEYS xy=k pool keys, got ${XYK_COUNT:-0}"
    exit 1
  fi
else
  echo "redis-cli not found — skip Redis checks"
fi

echo ""
echo "=== Quote (1 XLM -> USDC) ==="
QUOTE="${API_BASE}/api/v1/quote?token_in=${XLM}&token_out=${USDC}&amount_in=10000000&slippage=0.5"
QUOTE_BODY=$(curl -sf --max-time 45 "$QUOTE")
echo "$QUOTE_BODY" | head -c 600
echo ""
EXPECTED=$(echo "$QUOTE_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('expected_output','0'))" 2>/dev/null || echo "0")
if [[ "${EXPECTED:-0}" -lt 1 ]]; then
  echo "FAIL: quote expected_output=$EXPECTED"
  exit 1
fi
echo "Quote OK (expected_output=$EXPECTED)"

echo ""
echo "=== Soroban path participation (api log) ==="
if journalctl -u lumagg-api@3100 --since "3 min ago" --no-pager 2>/dev/null | grep -q "soroban_quoted"; then
  journalctl -u lumagg-api@3100 --since "3 min ago" --no-pager | grep -E "quote_route hydration|Paths quoted|Comparing classic" | tail -5 || true
else
  echo "WARN: no soroban_quoted log yet (trigger another quote)"
  curl -sf --max-time 45 "$QUOTE" >/dev/null || true
  sleep 1
  journalctl -u lumagg-api@3100 --since "1 min ago" --no-pager | grep -E "quote_route hydration|Paths quoted|Comparing classic" | tail -5 || true
fi

echo ""
echo "=== All checks passed ==="

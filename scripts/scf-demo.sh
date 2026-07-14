#!/usr/bin/env bash
# Quick demo script for SCF reviewers / local smoke test.
set -euo pipefail

API="${API:-https://api.lumagg.xyz}"
XLM="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"

echo "=== LumAgg SCF demo ==="
echo "API=$API"
echo

echo "=== 1. Health ==="
curl -sf "$API/api/v1/health" | python3 -m json.tool
echo

echo "=== 2. Quote: 1 XLM -> USDC ==="
QUOTE=$(curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "slippage=0.5")
echo "$QUOTE" | python3 -m json.tool
echo

echo "=== 3. Leg summary ==="
echo "$QUOTE" | python3 -c "
import json, sys
d = json.load(sys.stdin).get('data') or {}
routes = d.get('sub_routes') or []
total_in = int(d.get('amount_in') or 0)
print(f\"expected_output={d.get('expected_output')} is_split={d.get('is_split')} compute_ms={d.get('compute_time_ms')}\")
for i, r in enumerate(routes):
    ain, aout = int(r['amount_in']), int(r['amount_out'])
    pct_in = (ain * 10000 // total_in) / 100 if total_in else 0
    rate = aout / ain if ain else 0
    print(f\"  {i+1}. {r['source'][:50]:50} in_bps={pct_in:.2f}% rate={rate:.4f}\")
"

echo
echo "=== 4. Stats (when INDEXER_DB_PATH configured on API) ==="
curl -sf "$API/api/v1/stats" 2>/dev/null | python3 -m json.tool | head -25 || echo "(stats endpoint not configured yet)"
echo
echo "=== Done ==="
echo "Full integrator path: USER_G=G... ./scripts/integrator-smoke.sh"
echo "Arb evidence: ./scripts/collect-arb-evidence.sh"
echo "See docs/scf-build.md · docs/integrator-guide.md"

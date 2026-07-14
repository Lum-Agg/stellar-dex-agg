#!/usr/bin/env bash
# End-to-end integrator smoke: quote → build_tx (D2 evidence script).
#
# Usage:
#   USER_G=G... ./scripts/integrator-smoke.sh
#   API=http://127.0.0.1:3100 USER_G=G... ./scripts/integrator-smoke.sh
#
# Env:
#   API             default https://api.lumagg.xyz
#   USER_G          required — funded mainnet account (sequence on chain)
#   AMOUNT_IN       stroops, default 10000000 (1 XLM)
#   PREFER_SOROBAN  set to 1 for Soroban-only quote
set -euo pipefail

API="${API:-https://api.lumagg.xyz}"
XLM="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
AMOUNT_IN="${AMOUNT_IN:-10000000}"
USER_G="${USER_G:-}"

if [[ -z "$USER_G" ]]; then
  echo "ERROR: set USER_G to your Stellar public key (G...)" >&2
  echo "  USER_G=G... ./scripts/integrator-smoke.sh" >&2
  exit 1
fi

PREFER_ARG=()
if [[ "${PREFER_SOROBAN:-}" == "1" ]]; then
  PREFER_ARG=(--data-urlencode "prefer_soroban=1")
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
export TMP

echo "=== LumAgg integrator smoke ==="
echo "API=$API USER_G=${USER_G:0:8}… amount_in=$AMOUNT_IN"
echo

echo "=== 1. Health ==="
curl -sf "$API/api/v1/health" | python3 -m json.tool
echo

echo "=== 2. Quote ==="
curl -sfG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=$AMOUNT_IN" \
  --data-urlencode "slippage=0.5" \
  "${PREFER_ARG[@]}" > "$TMP/quote.json"
python3 -m json.tool "$TMP/quote.json" | head -40
echo "…"
echo

echo "=== 3. build_tx ==="
export USER_G XLM USDC AMOUNT_IN
python3 <<'PY' > "$TMP/build.json"
import json, os, sys

with open(os.environ["TMP"] + "/quote.json") as f:
    quote = json.load(f)
if not quote.get("success"):
    print(json.dumps({"error": quote.get("error", quote)}))
    sys.exit(1)

d = quote["data"]
xlm = os.environ["XLM"]
usdc = os.environ["USDC"]
amount_default = os.environ["AMOUNT_IN"]

body = {
    "user_public_key": os.environ["USER_G"],
    "token_in": xlm,
    "token_out": usdc,
    "amount_in": d.get("amount_in") or amount_default,
    "min_amount_out": d["minimum_output"],
    "sub_routes": [],
}
for sr in d.get("sub_routes") or []:
    path = sr.get("path") or []
    pools = sr.get("pool_addresses") or []
    dtypes = sr.get("dex_types") or []
    ins = sr.get("in_indices") or []
    outs = sr.get("out_indices") or []
    steps = []
    for i, pool in enumerate(pools):
        steps.append({
            "dex_type": dtypes[i] if i < len(dtypes) else "aquarius",
            "pool_address": pool,
            "token_in": path[i] if i < len(path) else xlm,
            "token_out": path[i + 1] if i + 1 < len(path) else usdc,
            "in_idx": int(ins[i]) if i < len(ins) else 0,
            "out_idx": int(outs[i]) if i < len(outs) else 1,
        })
    body["sub_routes"].append({"amount_in": sr["amount_in"], "steps": steps})
print(json.dumps(body))
PY

curl -sf -X POST "$API/api/v1/build_tx" \
  -H "Content-Type: application/json" \
  -d @"$TMP/build.json" > "$TMP/build_resp.json"
python3 -m json.tool "$TMP/build_resp.json"

XDR=$(python3 -c "import json; j=json.load(open('$TMP/build_resp.json')); print((j.get('data') or {}).get('unsigned_tx_xdr','')[:80])")
if [[ -n "$XDR" ]]; then
  echo
  echo "SUCCESS: unsigned_tx_xdr prefix: ${XDR}…"
  echo "Next: sign with Freighter / wallet and submit."
else
  echo "build_tx did not return XDR — check error above" >&2
  exit 1
fi

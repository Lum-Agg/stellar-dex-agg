#!/usr/bin/env bash
# Regression checks for quote sanity (large size, no fantasy legs).
set -euo pipefail

API="${API:-https://api.lumagg.xyz}"
XLM="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

quote() {
  local token_in=$1
  local token_out=$2
  local amount_in=$3
  curl -sfG "$API/api/v1/quote" \
    --data-urlencode "token_in=$token_in" \
    --data-urlencode "token_out=$token_out" \
    --data-urlencode "amount_in=$amount_in" \
    --data-urlencode "slippage=0.5"
}

check_quote() {
  local label=$1
  local token_in=$2
  local token_out=$3
  local amount_in=$4
  local json
  json=$(quote "$token_in" "$token_out" "$amount_in")

  echo "=== $label (amount_in=$amount_in) ==="
  python3 - "$json" "$amount_in" <<'PY'
import json, sys
raw, amount_in = sys.argv[1], int(sys.argv[2])
d = json.loads(raw)
if not d.get("success"):
    raise SystemExit(f"quote failed: {d.get('error')}")
data = d["data"]
routes = data["sub_routes"]
total_in = int(data["amount_in"])
total_out = int(data["expected_output"])
if total_in != amount_in:
    raise SystemExit(f"amount_in mismatch api={total_in} expected={amount_in}")
leg_in = sum(int(r["amount_in"]) for r in routes)
if leg_in != total_in:
    raise SystemExit(f"leg sum {leg_in} != total_in {total_in}")

rates = []
for i, r in enumerate(routes):
    ain, aout = int(r["amount_in"]), int(r["amount_out"])
    in_bps = ain * 10000 // total_in
    rate = aout / ain if ain else 0
    rates.append(rate)
    flag = ""
    if in_bps < 10:
        flag = " [DUST_IN]"
    if len(rates) > 1 and rate > max(rates) * 3:
        flag += " [RATE?]"
    print(f"  leg{i+1} {r['source'][:40]:40} in_bps={in_bps/100:.2f}% rate={rate:.4f}{flag}")

if not rates:
    raise SystemExit("no routes")
median = sorted(rates)[len(rates) // 2]
for rate in rates:
    if rate > median * 2.5:
        raise SystemExit(f"fantasy rate {rate:.4f} vs median {median:.4f}")
    if rate < median / 2.5 and median > 0.01:
        raise SystemExit(f"outlier low rate {rate:.4f} vs median {median:.4f}")

print(f"  OK legs={len(routes)} out={total_out} compute_ms={data.get('compute_time_ms')}")
PY
  echo
}

echo "LumAgg quote regression — API=$API"
echo
echo "Note: XLM→USDC often wins on classic_dex (Horizon path). USDC→XLM exercises Soroban routing."
echo

echo "--- XLM → USDC ---"
check_quote "1 XLM → USDC" "$XLM" "$USDC" "10000000"
check_quote "10 XLM → USDC" "$XLM" "$USDC" "100000000"
check_quote "1000 XLM → USDC" "$XLM" "$USDC" "10000000000"

echo "--- USDC → XLM (reverse) ---"
check_quote "1 USDC → XLM" "$USDC" "$XLM" "10000000"
check_quote "10 USDC → XLM" "$USDC" "$XLM" "100000000"
check_quote "1000 USDC → XLM" "$USDC" "$XLM" "10000000000"

echo "All regression checks passed."

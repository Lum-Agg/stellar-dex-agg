#!/usr/bin/env bash
# Simulate aggregator.swap one DEX at a time via POST /api/v1/build_tx (no submit, no XLM fee).
#
# Usage:
#   ./scripts/simulate_single_dex.sh                    # all cases in dex_simulate_cases.json
#   ./scripts/simulate_single_dex.sh soroswap_usdc_xlm  # one case id
#   API_BASE=http://127.0.0.1:8080 ./scripts/simulate_single_dex.sh
#
# Requires: curl, python3, jq (optional for pretty JSON)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASES_FILE="${CASES_FILE:-$ROOT/scripts/dex_simulate_cases.json}"
API_BASE="${API_BASE:-https://api.lumagg.xyz}"
USER_AGENT="${USER_AGENT:-Mozilla/5.0 (LumAggDexSimulate/1.0)}"
FILTER_ID="${1:-}"

if [[ ! -f "$CASES_FILE" ]]; then
  echo "Missing $CASES_FILE" >&2
  exit 1
fi

export CASES_FILE API_BASE FILTER_ID USER_AGENT
python3 <<'PY'
import json
import os
import sys
import urllib.request

cases_path = os.environ["CASES_FILE"]
api_base = os.environ["API_BASE"].rstrip("/")
filter_id = os.environ.get("FILTER_ID", "")
ua = os.environ["USER_AGENT"]

with open(cases_path) as f:
    cfg = json.load(f)

tokens = cfg["tokens"]
defaults = cfg["defaults"]
cases = cfg["cases"]
if filter_id:
    cases = [c for c in cases if c["id"] == filter_id]
    if not cases:
        print(f"No case with id={filter_id!r}", file=sys.stderr)
        sys.exit(1)

def resolve(tok):
    return tokens.get(tok, tok)

def post_build_tx(body: dict) -> tuple[bool, str]:
    url = f"{api_base}/api/v1/build_tx"
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json", "User-Agent": ua},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            raw = resp.read().decode()
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
    except Exception as e:
        return False, str(e)

    try:
        j = json.loads(raw)
    except json.JSONDecodeError:
        return False, raw[:500]

    if j.get("success"):
        d = j.get("data") or {}
        return True, f"unsigned_tx len={len(d.get('unsigned_tx_xdr') or '')} ops={d.get('num_operations')} exec={d.get('execution')}"
    err = j.get("error") or raw
    # Shorten Soroban diagnostic blobs
    if len(err) > 400:
        if "UnreachableCodeReached" in err:
            err = "UnreachableCodeReached (contract/pool trap during swap)"
        elif "SwapKConstantNotMet" in err or "K constant" in err:
            err = "SwapKConstantNotMet (soroswap amount_out vs reserves)"
        elif "Output below minimum" in err:
            err = "Output below minimum (slippage)"
        elif "EmptyPool" in err:
            err = "EmptyPool / insufficient liquidity"
        elif "transfer" in err and "Error(Contract" in err:
            err = "Token transfer auth/balance error"
        else:
            err = err[:400] + "..."
    return False, err

print(f"API: {api_base}")
print(f"Cases: {len(cases)} from {cases_path}\n")
print(f"{'ID':<28} {'DEX':<14} {'SIMULATE':<10} DETAIL")
print("-" * 90)

ok_count = 0
skip_count = 0
for c in cases:
    cid = c["id"]
    if c.get("skip"):
        skip_count += 1
        print(f"{cid:<28} {c['dex_type']:<14} {'SKIP':<10} {c.get('note', '')[:48]}")
        continue

    token_in = resolve(c["token_in"])
    token_out = resolve(c["token_out"])
    amount_in = str(c.get("amount_in", defaults["amount_in"]))
    min_out = str(c.get("min_amount_out", defaults["min_amount_out"]))
    user = defaults["user_public_key"]

    body = {
        "user_public_key": user,
        "token_in": token_in,
        "token_out": token_out,
        "amount_in": amount_in,
        "min_amount_out": min_out,
        "sub_routes": [
            {
                "amount_in": amount_in,
                "steps": [
                    {
                        "dex_type": c["dex_type"],
                        "pool_address": c["pool_address"],
                        "token_in": token_in,
                        "token_out": token_out,
                        "in_idx": c["in_idx"],
                        "out_idx": c["out_idx"],
                    }
                ],
            }
        ],
    }

    ok, detail = post_build_tx(body)
    status = "OK" if ok else "FAIL"
    if ok:
        ok_count += 1
    note = (c.get("note") or "")[:36]
    print(f"{cid:<28} {c['dex_type']:<14} {status:<10} {detail}")
    if note:
        print(f"{'':28} {'':14} {'':10} ({note})")

print("-" * 90)
run = len(cases) - skip_count
print(f"Simulate OK: {ok_count}/{run}  (skipped {skip_count})")
print("\nTip: edit scripts/dex_simulate_cases.json to add pools or change amount_in.")
PY

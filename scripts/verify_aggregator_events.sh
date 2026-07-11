#!/usr/bin/env bash
# Verify aggregator contract events after WASM upgrade.
#
# Usage:
#   ./scripts/verify_aggregator_events.sh
#   RPC_URL=https://mainnet.sorobanrpc.com START_LEDGER=63200000 ./scripts/verify_aggregator_events.sh
#
# Checks getEvents for swap/rt/leg topics on the mainnet aggregator contract.

set -euo pipefail

AGGREGATOR="${AGGREGATOR:-CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K}"
RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
START_LEDGER="${START_LEDGER:-}"
LIMIT="${LIMIT:-200}"

if [[ -z "$START_LEDGER" ]]; then
  START_LEDGER=$(curl -sS -X POST "$RPC_URL" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger","params":{}}' \
    | python3 -c "import sys,json; r=json.load(sys.stdin); print(int(r['result']['sequence'])-500)")
fi

echo "=== Aggregator event check ==="
echo "contract: $AGGREGATOR"
echo "rpc:      $RPC_URL"
echo "start:    $START_LEDGER (latest-500 if unset)"
echo ""

payload=$(python3 - <<PY
import json
print(json.dumps({
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getEvents",
  "params": {
    "startLedger": int("$START_LEDGER"),
    "filters": [{"type": "contract", "contractIds": ["$AGGREGATOR"]}],
    "pagination": {"limit": int("$LIMIT")}
  }
}))
PY
)

resp=$(curl -sS -X POST "$RPC_URL" -H 'Content-Type: application/json' -d "$payload")
echo "$resp" | python3 - <<'PY'
import json, sys, base64
from collections import Counter

data = json.load(sys.stdin)
events = data.get("result", {}).get("events", [])
topics = Counter()
for ev in events:
    if ev.get("type") != "contract":
        continue
    t = ev.get("topic") or []
    if not t:
        continue
    try:
        raw = base64.b64decode(t[0])
        # ScSymbol: 4-byte len prefix + utf8
        if len(raw) >= 4:
            n = int.from_bytes(raw[:4], "big")
            sym = raw[4:4+n].decode("utf-8", errors="replace")
            topics[sym] += 1
    except Exception:
        topics["<decode-error>"] += 1

print(f"events returned: {len(events)}")
if not topics:
    print("no contract events found in window — upgrade may not be live yet, or no swaps in range")
else:
    print("topic counts:")
    for k, v in sorted(topics.items()):
        print(f"  {k}: {v}")
    if any(k in topics for k in ("swap", "rt", "leg")):
        print("\n✓ LumAgg aggregator events detected")
    else:
        print("\n⚠ contract events exist but no swap/rt/leg topics yet")
PY

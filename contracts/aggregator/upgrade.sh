#!/usr/bin/env bash
# Build optimized WASM and upgrade the deployed aggregator contract on mainnet.
#
# Prerequisites:
#   - stellar CLI (https://developers.stellar.org/docs/tools/cli)
#   - Admin key configured: stellar keys add admin --source-file ...
#
# Usage:
#   ./contracts/aggregator/upgrade.sh
#   ADMIN=admin AGGREGATOR=CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K ./contracts/aggregator/upgrade.sh
#
# Stellar CLI 25+ ships mainnet with a docs placeholder RPC URL (not usable).
# Override RPC if needed:
#   RPC_URL=https://mainnet.sorobanrpc.com ./contracts/aggregator/upgrade.sh
#
# CLI waits ~30s for ledger inclusion; upload on mainnet can take longer.
# On "transaction submission timeout", this script polls the chain (TX_POLL_SECS).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/contracts/aggregator"

ADMIN="${ADMIN:-admin}"
AGGREGATOR="${AGGREGATOR:-CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
TX_POLL_SECS="${TX_POLL_SECS:-180}"
# Prefer simulated resource fee (do not override unless needed). Manually setting
# --resource-fee too low/high can cause InsufficientRefundableFee / fee-bump fails.
INCLUSION_FEE="${INCLUSION_FEE:-1000000}"
INSTRUCTION_LEEWAY="${INSTRUCTION_LEEWAY:-5000000}"

compute_wasm_hash() {
  openssl dgst -sha256 "$1" | awk '{print $2}'
}

poll_tx() {
  local tx_hash="$1"
  local deadline=$((SECONDS + TX_POLL_SECS))
  echo "Polling getTransaction (up to ${TX_POLL_SECS}s): $tx_hash"
  echo "  https://stellar.expert/explorer/public/tx/$tx_hash"
  while (( SECONDS < deadline )); do
    local resp status
    resp=$(curl -sS -X POST "$RPC_URL" \
      -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":{\"hash\":\"$tx_hash\"}}")
    status=$(echo "$resp" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r.get('result',{}).get('status',''))" 2>/dev/null || true)
    case "$status" in
      SUCCESS)
        echo "Transaction succeeded on ledger."
        return 0
        ;;
      FAILED)
        echo "Transaction FAILED on ledger."
        echo "$resp" | python3 -m json.tool 2>/dev/null || echo "$resp"
        return 1
        ;;
    esac
    sleep 3
  done
  echo "Still not finalized after ${TX_POLL_SECS}s — check the explorer link above."
  return 2
}

# Run stellar upload/invoke; on CLI submission timeout, poll by tx hash and continue if SUCCESS.
run_stellar_tx() {
  local label="$1"
  shift
  local log
  log=$(mktemp)
  set +e
  "$@" 2>&1 | tee "$log"
  local ec=${PIPESTATUS[0]}
  set -e

  if [[ $ec -eq 0 ]]; then
    rm -f "$log"
    return 0
  fi

  if grep -qi "submission timeout" "$log"; then
    local tx_hash
    tx_hash=$(grep -oE '[0-9a-f]{64}' "$log" | head -1 || true)
    rm -f "$log"
    if [[ -n "$tx_hash" ]]; then
      echo "⚠️  $label: CLI timed out after ~30s; checking chain for tx $tx_hash ..."
      if poll_tx "$tx_hash"; then
        return 0
      fi
      echo "If the explorer shows SUCCESS, re-run this script (WASM may already be installed)."
      return 2
    fi
  fi

  echo "❌ $label failed (exit $ec)."
  cat "$log" >&2
  rm -f "$log"
  return "$ec"
}

echo "=== Building aggregator WASM (contract-release) ==="
stellar contract build --package aggregator-contract --profile contract-release --optimize 2>/dev/null || {
  echo "stellar contract build --optimize failed, trying cargo + stellar optimize..."
  cargo build -p aggregator-contract --target wasm32v1-none --profile contract-release
  WASM=""
  for CANDIDATE in \
    "$ROOT/target/wasm32v1-none/contract-release/aggregator_contract.wasm" \
    "$ROOT/contracts/aggregator/target/wasm32v1-none/contract-release/aggregator_contract.wasm" \
    "$ROOT/target/wasm32v1-none/release/aggregator_contract.wasm" \
    "$ROOT/contracts/aggregator/target/wasm32v1-none/release/aggregator_contract.wasm" \
    "$ROOT/target/wasm32-unknown-unknown/release/aggregator_contract.wasm" \
    "$ROOT/contracts/aggregator/target/wasm32-unknown-unknown/release/aggregator_contract.wasm"
  do
    if [[ -f "$CANDIDATE" ]]; then
      WASM="$CANDIDATE"
      break
    fi
  done
  if [[ -z "$WASM" ]]; then
    echo "ERROR: Could not find built WASM after cargo build."
    exit 1
  fi
  stellar contract optimize --wasm "$WASM" --wasm-out "$WASM"
}

WASM=""
for CANDIDATE in \
  "$ROOT/target/wasm32v1-none/contract-release/aggregator_contract.wasm" \
  "$ROOT/contracts/aggregator/target/wasm32v1-none/contract-release/aggregator_contract.wasm" \
  "$ROOT/target/wasm32v1-none/release/aggregator_contract.wasm" \
  "$ROOT/contracts/aggregator/target/wasm32v1-none/release/aggregator_contract.wasm" \
  "$ROOT/target/wasm32-unknown-unknown/release/aggregator_contract.wasm" \
  "$ROOT/contracts/aggregator/target/wasm32-unknown-unknown/release/aggregator_contract.wasm"
do
  if [[ -f "$CANDIDATE" ]]; then
    WASM="$CANDIDATE"
    break
  fi
done
if [[ ! -f "$WASM" ]]; then
  echo "ERROR: WASM not found under target/"
  exit 1
fi
echo "WASM: $WASM ($(wc -c < "$WASM") bytes)"

WASM_HASH=$(compute_wasm_hash "$WASM")
echo "WASM hash (sha256): $WASM_HASH"

echo "=== Uploading WASM via RPC ($RPC_URL) ==="
UPLOAD_ARGS=(
  --rpc-url "$RPC_URL"
  --network-passphrase "$NETWORK_PASSPHRASE"
  --source-account "$ADMIN"
  --inclusion-fee "$INCLUSION_FEE"
  --instruction-leeway "$INSTRUCTION_LEEWAY"
  --wasm "$WASM"
)
# Optional override only when explicitly set.
if [[ -n "${RESOURCE_FEE:-}" ]]; then
  UPLOAD_ARGS+=(--resource-fee "$RESOURCE_FEE")
fi
if ! run_stellar_tx "WASM upload" stellar contract upload "${UPLOAD_ARGS[@]}"; then
  echo ""
  echo "Upload did not confirm. You can retry with another RPC:"
  echo "  RPC_URL=https://soroban-rpc.mainnet.stellar.gateway.fm TX_POLL_SECS=300 ./contracts/aggregator/upgrade.sh"
  echo "Or only upgrade if upload already succeeded:"
  echo "  stellar contract invoke --id $AGGREGATOR --source $ADMIN \\"
  echo "    --rpc-url $RPC_URL --network-passphrase \"$NETWORK_PASSPHRASE\" \\"
  echo "    -- upgrade --new_wasm_hash $WASM_HASH"
  exit 1
fi

echo "=== Upgrading contract $AGGREGATOR ==="
UPGRADE_ARGS=(
  --id "$AGGREGATOR"
  --source-account "$ADMIN"
  --rpc-url "$RPC_URL"
  --network-passphrase "$NETWORK_PASSPHRASE"
  --inclusion-fee "$INCLUSION_FEE"
  --instruction-leeway "$INSTRUCTION_LEEWAY"
)
if [[ -n "${UPGRADE_RESOURCE_FEE:-}" ]]; then
  UPGRADE_ARGS+=(--resource-fee "$UPGRADE_RESOURCE_FEE")
fi
if ! run_stellar_tx "contract upgrade" stellar contract invoke \
  "${UPGRADE_ARGS[@]}" \
  -- \
  upgrade \
  --new_wasm_hash "$WASM_HASH"; then
  exit 1
fi

echo "=== Done ==="
echo "Aggregator upgraded to WASM hash $WASM_HASH"
echo "Verify with a small simulate swap on split routes."

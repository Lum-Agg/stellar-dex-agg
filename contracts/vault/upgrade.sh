#!/usr/bin/env bash
# Build optimized WASM and upgrade a deployed LumAgg arb vault contract.
#
# Usage:
#   VAULT=C... ./contracts/vault/upgrade.sh
#   VAULT=$(cat contracts/vault/.mainnet-vault-id) ./contracts/vault/upgrade.sh
#
# See contracts/vault/deploy.sh for RPC / timeout env vars.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/contracts/vault"

ADMIN="${ADMIN:-admin}"
VAULT="${VAULT:-}"
if [[ -z "$VAULT" && -f "$ROOT/contracts/vault/.mainnet-vault-id" ]]; then
  VAULT=$(cat "$ROOT/contracts/vault/.mainnet-vault-id")
fi
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
TX_POLL_SECS="${TX_POLL_SECS:-180}"

if [[ -z "$VAULT" ]]; then
  echo "ERROR: Set VAULT to the deployed contract id (C...)."
  exit 1
fi

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
      SUCCESS) return 0 ;;
      FAILED)
        echo "Transaction FAILED on ledger."
        echo "$resp" | python3 -m json.tool 2>/dev/null || echo "$resp"
        return 1
        ;;
    esac
    sleep 3
  done
  return 2
}

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
    if [[ -n "$tx_hash" ]] && poll_tx "$tx_hash"; then
      return 0
    fi
    return 2
  fi
  echo "❌ $label failed (exit $ec)."
  cat "$log" >&2
  rm -f "$log"
  return "$ec"
}

echo "=== Building vault WASM (release) ==="
stellar contract build --release --optimize 2>/dev/null || {
  cargo build -p vault-contract --target wasm32v1-none --profile contract-release
  WASM=""
  for CANDIDATE in \
    "$ROOT/target/wasm32v1-none/contract-release/vault_contract.wasm" \
    "$ROOT/contracts/vault/target/wasm32v1-none/contract-release/vault_contract.wasm" \
    "$ROOT/target/wasm32v1-none/release/vault_contract.wasm" \
    "$ROOT/contracts/vault/target/wasm32v1-none/release/vault_contract.wasm"
  do
    [[ -f "$CANDIDATE" ]] && WASM="$CANDIDATE" && break
  done
  [[ -n "$WASM" ]] || { echo "ERROR: WASM not found"; exit 1; }
  stellar contract optimize --wasm "$WASM" --wasm-out "$WASM"
}

WASM=""
for CANDIDATE in \
  "$ROOT/target/wasm32v1-none/contract-release/vault_contract.wasm" \
  "$ROOT/contracts/vault/target/wasm32v1-none/contract-release/vault_contract.wasm" \
  "$ROOT/target/wasm32v1-none/release/vault_contract.wasm" \
  "$ROOT/contracts/vault/target/wasm32v1-none/release/vault_contract.wasm" \
  "$ROOT/target/wasm32-unknown-unknown/release/vault_contract.wasm"
do
  [[ -f "$CANDIDATE" ]] && WASM="$CANDIDATE" && break
done
[[ -f "$WASM" ]] || { echo "ERROR: WASM not found"; exit 1; }

WASM_HASH=$(compute_wasm_hash "$WASM")
echo "WASM hash: $WASM_HASH"

echo "=== Uploading WASM ==="
run_stellar_tx "WASM upload" stellar contract upload \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --source "$ADMIN" \
  --wasm "$WASM"

echo "=== Upgrading vault $VAULT ==="
run_stellar_tx "contract upgrade" stellar contract invoke \
  --id "$VAULT" \
  --source "$ADMIN" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- \
  upgrade \
  --new_wasm_hash "$WASM_HASH"

echo "=== Done ==="
echo "Vault upgraded to WASM hash $WASM_HASH"

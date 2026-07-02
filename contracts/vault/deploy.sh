#!/usr/bin/env bash
# Build optimized WASM and deploy a new LumAgg arb vault on mainnet (or testnet).
#
# Prerequisites:
#   - stellar CLI (https://developers.stellar.org/docs/tools/cli)
#   - Admin key configured: stellar keys add admin --source-file ...
#
# Usage:
#   ./contracts/vault/deploy.sh
#   ADMIN=admin ADMIN_G=G... ./contracts/vault/deploy.sh
#   CALLER=G... ./contracts/vault/deploy.sh   # optional: add_caller after deploy
#
# Stellar CLI 25+ ships mainnet with a docs placeholder RPC URL (not usable).
# Override RPC if needed:
#   RPC_URL=https://soroban-rpc.mainnet.stellar.gateway.fm ./contracts/vault/deploy.sh
#
# On "transaction submission timeout", this script polls the chain (TX_POLL_SECS).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/contracts/vault"

ADMIN="${ADMIN:-admin}"
ADMIN_G="${ADMIN_G:-}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
TX_POLL_SECS="${TX_POLL_SECS:-180}"
CALLER="${CALLER:-}"
AGGREGATOR="${AGGREGATOR:-CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K}"

if [[ -z "$ADMIN_G" ]]; then
  ADMIN_G=$(stellar keys address "$ADMIN" 2>/dev/null || true)
fi
if [[ -z "$ADMIN_G" ]]; then
  echo "ERROR: Set ADMIN_G to the admin public key (G-address) for initialize()."
  echo "  ADMIN_G=G... ./contracts/vault/deploy.sh"
  exit 1
fi

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
      return 2
    fi
  fi

  echo "❌ $label failed (exit $ec)."
  cat "$log" >&2
  rm -f "$log"
  return "$ec"
}

echo "=== Building vault WASM (release) ==="
stellar contract build --release --optimize 2>/dev/null || {
  echo "stellar contract build --optimize failed, trying cargo + stellar optimize..."
  cargo build -p vault-contract --target wasm32v1-none --release
  WASM=""
  for CANDIDATE in \
    "$ROOT/target/wasm32v1-none/release/vault_contract.wasm" \
    "$ROOT/contracts/vault/target/wasm32v1-none/release/vault_contract.wasm" \
    "$ROOT/target/wasm32-unknown-unknown/release/vault_contract.wasm" \
    "$ROOT/contracts/vault/target/wasm32-unknown-unknown/release/vault_contract.wasm"
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
  "$ROOT/target/wasm32v1-none/release/vault_contract.wasm" \
  "$ROOT/contracts/vault/target/wasm32v1-none/release/vault_contract.wasm" \
  "$ROOT/target/wasm32-unknown-unknown/release/vault_contract.wasm" \
  "$ROOT/contracts/vault/target/wasm32-unknown-unknown/release/vault_contract.wasm"
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

echo "=== Deploying vault (initialize admin=$ADMIN_G) via RPC ($RPC_URL) ==="
DEPLOY_LOG=$(mktemp)
set +e
stellar contract deploy \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --source "$ADMIN" \
  --wasm "$WASM" \
  -- \
  initialize --admin "$ADMIN_G" 2>&1 | tee "$DEPLOY_LOG"
DEPLOY_EC=${PIPESTATUS[0]}
set -e

VAULT_ID=""
if [[ $DEPLOY_EC -eq 0 ]]; then
  VAULT_ID=$(grep -oE 'C[A-Z2-7]{55}' "$DEPLOY_LOG" | tail -1 || true)
else
  if grep -qi "submission timeout" "$DEPLOY_LOG"; then
    TX_HASH=$(grep -oE '[0-9a-f]{64}' "$DEPLOY_LOG" | head -1 || true)
    if [[ -n "$TX_HASH" ]] && poll_tx "$TX_HASH"; then
      echo "Deploy tx succeeded but contract id not in CLI output — check explorer for created contract."
    fi
  fi
  rm -f "$DEPLOY_LOG"
  echo "Deploy failed (exit $DEPLOY_EC)."
  exit "$DEPLOY_EC"
fi
rm -f "$DEPLOY_LOG"

if [[ -z "$VAULT_ID" ]]; then
  echo "WARNING: Could not parse contract id from CLI output. Check stellar CLI logs."
else
  echo ""
  echo "=== Vault deployed ==="
  echo "VAULT=$VAULT_ID"
  echo "  https://stellar.expert/explorer/public/contract/$VAULT_ID"
  echo "$VAULT_ID" > "$ROOT/contracts/vault/.mainnet-vault-id"
  echo "(saved to contracts/vault/.mainnet-vault-id — gitignored if you add it)"
fi

if [[ -n "$CALLER" && -n "$VAULT_ID" ]]; then
  IFS=',' read -ra CALLERS <<< "$CALLER"
  for c in "${CALLERS[@]}"; do
    c=$(echo "$c" | xargs)
    [[ -z "$c" ]] && continue
    echo "=== add_caller $c ==="
    run_stellar_tx "add_caller" stellar contract invoke \
      --id "$VAULT_ID" \
      --source "$ADMIN" \
      --rpc-url "$RPC_URL" \
      --network-passphrase "$NETWORK_PASSPHRASE" \
      -- \
      add_caller --caller "$c"
  done
fi

echo ""
echo "=== Next steps ==="
echo "1. deposit trading principal into the vault (XLM SAC or base token):"
echo "   stellar contract invoke --id $VAULT_ID --source <funder> \\"
echo "     --rpc-url $RPC_URL --network-passphrase \"$NETWORK_PASSPHRASE\" \\"
echo "     -- deposit --from <funder> --token <token_C...> --amount <stroops>"
echo "2. add_caller for each arb bot G-address (if not done via CALLER=...):"
echo "   stellar contract invoke --id $VAULT_ID --source $ADMIN \\"
echo "     --rpc-url $RPC_URL --network-passphrase \"$NETWORK_PASSPHRASE\" \\"
echo "     -- add_caller --caller G..."
echo "3. Run arb bot with vault mode:"
echo "   export ARB_VAULT_CONTRACT=$VAULT_ID"
echo "   export ARB_AGGREGATOR_CONTRACT=$AGGREGATOR"
echo "   export ARB_BUILD_TX=1 ARB_SUBMIT_TX=1"
echo "4. Future WASM upgrades: ./contracts/vault/upgrade.sh"

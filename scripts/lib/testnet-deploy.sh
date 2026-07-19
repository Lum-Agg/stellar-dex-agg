#!/usr/bin/env bash
# Shared helpers for testnet-only Soroban deploys.
# Source from deploy scripts:  source "$ROOT/scripts/lib/testnet-deploy.sh"

refuse_if_mainnet() {
  case "${NETWORK_PASSPHRASE:-}" in
    *"Public Global Stellar Network"*)
      echo "ERROR: This script is testnet-only. Refusing mainnet passphrase." >&2
      echo "  NETWORK_PASSPHRASE=${NETWORK_PASSPHRASE}" >&2
      exit 1
      ;;
  esac
}

set_testnet_defaults() {
  NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
  RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
  TX_POLL_SECS="${TX_POLL_SECS:-180}"
  RESOURCE_FEE="${RESOURCE_FEE:-200000000}"
  INCLUSION_FEE="${INCLUSION_FEE:-5000000}"
  INVOKE_RESOURCE_FEE="${INVOKE_RESOURCE_FEE:-50000000}"
  refuse_if_mainnet
}

resolve_admin_g() {
  local admin="${ADMIN:-admin}"
  if [[ -z "${ADMIN_G:-}" ]]; then
    ADMIN_G=$(stellar keys address "$admin" 2>/dev/null || true)
  fi
  if [[ -z "${ADMIN_G:-}" ]]; then
    echo "ERROR: Set ADMIN_G to the admin public key (G-address)." >&2
    echo "  ADMIN_G=G... ADMIN=$admin $0" >&2
    exit 1
  fi
}

poll_tx() {
  local tx_hash="$1"
  local deadline=$((SECONDS + TX_POLL_SECS))
  echo "Polling getTransaction (up to ${TX_POLL_SECS}s): $tx_hash"
  echo "  https://stellar.expert/explorer/testnet/tx/$tx_hash"
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

find_wasm() {
  local name="$1"
  local root="$2"
  local candidate
  for candidate in \
    "$root/target/wasm32v1-none/contract-release/${name}.wasm" \
    "$root/target/wasm32v1-none/release/${name}.wasm" \
    "$root/target/wasm32-unknown-unknown/release/${name}.wasm"
  do
    if [[ -f "$candidate" ]]; then
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

build_contract_wasm() {
  local package="$1"
  local wasm_stem="$2"
  local crate_dir="$3"
  local root="$4"

  echo "=== Building $package WASM (release) ===" >&2
  (
    cd "$crate_dir"
    if ! stellar contract build --optimize 2>/dev/null; then
      echo "stellar contract build --optimize failed, trying cargo + stellar optimize..." >&2
      cargo build -p "$package" --target wasm32v1-none --profile contract-release
      local wasm
      wasm=$(find_wasm "$wasm_stem" "$root") || {
        echo "ERROR: Could not find built WASM for $wasm_stem after cargo build." >&2
        exit 1
      }
      stellar contract optimize --wasm "$wasm" --wasm-out "$wasm"
    fi
  )

  local out
  out=$(find_wasm "$wasm_stem" "$root") || {
    echo "ERROR: WASM not found under target/ for $wasm_stem" >&2
    exit 1
  }
  echo "WASM: $out ($(wc -c < "$out") bytes)" >&2
  printf '%s\n' "$out"
}

deploy_wasm() {
  local wasm="$1"
  local admin="${ADMIN:-admin}"
  local deploy_log
  deploy_log=$(mktemp)
  echo "=== Deploying via RPC ($RPC_URL) ===" >&2
  set +e
  stellar contract deploy \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --source-account "$admin" \
    --resource-fee "$RESOURCE_FEE" \
    --inclusion-fee "$INCLUSION_FEE" \
    --wasm "$wasm" 2>&1 | tee "$deploy_log" >&2
  local deploy_ec=${PIPESTATUS[0]}
  set -e

  local contract_id=""
  if [[ $deploy_ec -eq 0 ]]; then
    contract_id=$(grep -oE 'C[A-Z2-7]{55}' "$deploy_log" | tail -1 || true)
  else
    if grep -qi "submission timeout" "$deploy_log"; then
      local tx_hash
      tx_hash=$(grep -oE '[0-9a-f]{64}' "$deploy_log" | head -1 || true)
      if [[ -n "$tx_hash" ]] && poll_tx "$tx_hash"; then
        echo "Deploy tx succeeded but contract id not in CLI output — check explorer for created contract." >&2
      fi
    fi
    rm -f "$deploy_log"
    echo "Deploy failed (exit $deploy_ec)." >&2
    exit "$deploy_ec"
  fi
  rm -f "$deploy_log"

  if [[ -z "$contract_id" ]]; then
    echo "ERROR: Could not parse contract id from CLI output." >&2
    exit 1
  fi
  printf '%s\n' "$contract_id"
}

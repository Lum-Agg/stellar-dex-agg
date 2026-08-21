#!/usr/bin/env bash
# Upgrade an existing LumAgg Aggregator on Stellar testnet only.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/testnet-deploy.sh"
set_testnet_defaults

ADMIN="${ADMIN:-admin}"
AGGREGATOR="${AGGREGATOR:-}"
if [[ -z "$AGGREGATOR" && -f "$ROOT/contracts/aggregator/.testnet-aggregator-id" ]]; then
  AGGREGATOR=$(tr -d '[:space:]' < "$ROOT/contracts/aggregator/.testnet-aggregator-id")
fi
if [[ ! "$AGGREGATOR" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "ERROR: AGGREGATOR must be a testnet contract id (C…, 56 chars)." >&2
  exit 1
fi

WASM=$(build_contract_wasm "aggregator-contract" "aggregator_contract" \
  "$ROOT/contracts/aggregator" "$ROOT")
WASM_HASH=$(openssl dgst -sha256 "$WASM" | awk '{print $2}')
echo "WASM hash: $WASM_HASH"

stellar contract upload \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --source-account "$ADMIN" \
  --resource-fee "$RESOURCE_FEE" \
  --inclusion-fee "$INCLUSION_FEE" \
  --wasm "$WASM"

run_stellar_tx "aggregator upgrade" stellar contract invoke \
  --id "$AGGREGATOR" \
  --source-account "$ADMIN" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --resource-fee "$INVOKE_RESOURCE_FEE" \
  --inclusion-fee "$INCLUSION_FEE" \
  -- \
  upgrade --new_wasm_hash "$WASM_HASH"

echo "Aggregator testnet upgrade complete: $AGGREGATOR"

#!/usr/bin/env bash
# Upgrade an existing LumAgg Order Escrow on Stellar testnet only.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/testnet-deploy.sh"
set_testnet_defaults

ADMIN="${ADMIN:-admin}"
ESCROW="${ESCROW:-}"
if [[ -z "$ESCROW" && -f "$ROOT/contracts/order-escrow/.testnet-escrow-id" ]]; then
  ESCROW=$(tr -d '[:space:]' < "$ROOT/contracts/order-escrow/.testnet-escrow-id")
fi
if [[ ! "$ESCROW" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "ERROR: ESCROW must be a testnet contract id (C…, 56 chars)." >&2
  exit 1
fi

WASM=$(build_contract_wasm "order-escrow-contract" "order_escrow_contract" \
  "$ROOT/contracts/order-escrow" "$ROOT")
WASM_HASH=$(openssl dgst -sha256 "$WASM" | awk '{print $2}')
echo "WASM hash: $WASM_HASH"

stellar contract upload \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --source-account "$ADMIN" \
  --resource-fee "$RESOURCE_FEE" \
  --inclusion-fee "$INCLUSION_FEE" \
  --wasm "$WASM"

run_stellar_tx "escrow upgrade" stellar contract invoke \
  --id "$ESCROW" \
  --source-account "$ADMIN" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --resource-fee "$INVOKE_RESOURCE_FEE" \
  --inclusion-fee "$INCLUSION_FEE" \
  -- \
  upgrade --new_wasm_hash "$WASM_HASH"

echo "Order Escrow testnet upgrade complete: $ESCROW"

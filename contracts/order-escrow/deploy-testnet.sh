#!/usr/bin/env bash
# Build optimized WASM and deploy LumAgg order-escrow on Stellar **testnet**.
#
# Hard rule: refuses mainnet / Public Global passphrase.
#
# Prerequisites:
#   - stellar CLI + funded testnet admin key
#   - KEEPER_G: public G-address of the keeper account
#   - Testnet aggregator already deployed (AGGREGATOR=C… or .testnet-aggregator-id)
#
# Usage:
#   KEEPER_G=G... AGGREGATOR=C... ./contracts/order-escrow/deploy-testnet.sh
#   ESCROW_ASSETS=C...,C...  # optional comma-separated asset contract IDs

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../../scripts/lib/testnet-deploy.sh
source "$ROOT/scripts/lib/testnet-deploy.sh"

set_testnet_defaults
require_keeper_g
ADMIN="${ADMIN:-admin}"
resolve_admin_g

ID_FILE="$ROOT/contracts/order-escrow/.testnet-escrow-id"
AGG_ID_FILE="$ROOT/contracts/aggregator/.testnet-aggregator-id"
AGG_PIN_FILE="$ROOT/contracts/aggregator/.testnet-aggregator-pin"

if [[ -z "${AGGREGATOR:-}" && -f "$AGG_ID_FILE" ]]; then
  AGGREGATOR=$(tr -d '[:space:]' < "$AGG_ID_FILE")
fi
if [[ -z "${AGGREGATOR:-}" ]]; then
  echo "ERROR: Set AGGREGATOR=C... (testnet aggregator) or run aggregator deploy-testnet.sh first." >&2
  exit 1
fi
if [[ ! "$AGGREGATOR" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "ERROR: AGGREGATOR must be a contract id (C…, 56 chars)." >&2
  exit 1
fi

ASSETS=()
if [[ -n "${ESCROW_ASSETS:-}" ]]; then
  if [[ ! "$ESCROW_ASSETS" =~ ^C[A-Z2-7]{55}(,C[A-Z2-7]{55})*$ ]]; then
    echo "ERROR: ESCROW_ASSETS must be a comma-separated list of contract IDs." >&2
    exit 1
  fi
  IFS=, read -r -a ASSETS <<< "$ESCROW_ASSETS"
fi

resolve_aggregator_wasm_hash "$AGGREGATOR" "$AGG_PIN_FILE"
verify_testnet_aggregator "$AGGREGATOR" "$AGGREGATOR_WASM_HASH"

WASM=$(build_contract_wasm "order-escrow-contract" "order_escrow_contract" \
  "$ROOT")

CONTRACT_ID=$(deploy_wasm "$WASM" --optimize=false -- --admin "$ADMIN_G" --aggregator "$AGGREGATOR" --keeper "$KEEPER_G")
echo ""
echo "=== Order escrow deployed ==="
echo "ESCROW=$CONTRACT_ID"
echo "KEEPER_G=$KEEPER_G"
echo "  https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
echo "$CONTRACT_ID" > "$ID_FILE"
echo "(saved to $ID_FILE)"

if [[ -z "${ESCROW_ASSETS:-}" ]]; then
  echo "Asset allowlist is empty. Approve vetted assets with set_asset before creating orders."
else
  for asset in "${ASSETS[@]}"; do
    run_stellar_tx "approve escrow asset $asset" stellar contract invoke \
      --id "$CONTRACT_ID" \
      --source-account "$ADMIN" \
      --rpc-url "$RPC_URL" \
      --network-passphrase "$NETWORK_PASSPHRASE" \
      --resource-fee "$INVOKE_RESOURCE_FEE" \
      --inclusion-fee "$INCLUSION_FEE" \
      -- set_asset --asset "$asset" --allowed true
  done
fi

echo ""
echo "=== Done ==="
echo "export ESCROW_CONTRACT=$CONTRACT_ID"
echo "export AGGREGATOR_CONTRACT=$AGGREGATOR"
echo "See docs/limit-orders-testnet.md for smoke checklist."

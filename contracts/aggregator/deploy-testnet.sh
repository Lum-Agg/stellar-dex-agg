#!/usr/bin/env bash
# Build optimized WASM and deploy LumAgg aggregator on Stellar **testnet**.
#
# Hard rule: refuses mainnet / Public Global passphrase.
#
# Prerequisites:
#   - stellar CLI
#   - Admin key: stellar keys add admin --secret-key ... (testnet-funded)
#
# Usage:
#   ./contracts/aggregator/deploy-testnet.sh
#   ADMIN=admin ADMIN_G=G... ./contracts/aggregator/deploy-testnet.sh
#   AGGREGATOR=C... AGGREGATOR_WASM_HASH=... ./contracts/aggregator/deploy-testnet.sh
#
# Env overrides (still must be testnet):
#   RPC_URL=https://soroban-testnet.stellar.org

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../../scripts/lib/testnet-deploy.sh
source "$ROOT/scripts/lib/testnet-deploy.sh"

set_testnet_defaults
ADMIN="${ADMIN:-admin}"
resolve_admin_g

ID_FILE="$ROOT/contracts/aggregator/.testnet-aggregator-id"
PIN_FILE="$ROOT/contracts/aggregator/.testnet-aggregator-pin"

if [[ -n "${AGGREGATOR:-}" ]]; then
  if [[ ! "$AGGREGATOR" =~ ^C[A-Z2-7]{55}$ ]]; then
    echo "ERROR: AGGREGATOR must be a contract id (C…, 56 chars)." >&2
    exit 1
  fi
  resolve_aggregator_wasm_hash "$AGGREGATOR" "$PIN_FILE"
  verify_testnet_aggregator "$AGGREGATOR" "$AGGREGATOR_WASM_HASH"
  echo "=== Reusing verified aggregator (skip deploy) ==="
  echo "AGGREGATOR=$AGGREGATOR"
  echo "$AGGREGATOR" > "$ID_FILE"
  printf '%s %s\n' "$AGGREGATOR" "$AGGREGATOR_WASM_HASH" > "$PIN_FILE"
  echo "Saved to $ID_FILE"
  exit 0
fi

WASM=$(build_contract_wasm "aggregator-contract" "aggregator_contract" \
  "$ROOT")

EXPECTED_HASH=$(stellar contract info hash --wasm "$WASM")
CONTRACT_ID=$(deploy_wasm "$WASM" --optimize=false -- --admin "$ADMIN_G")
verify_testnet_aggregator "$CONTRACT_ID" "$EXPECTED_HASH"
echo ""
echo "=== Aggregator deployed ==="
echo "AGGREGATOR=$CONTRACT_ID"
echo "  https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
echo "$CONTRACT_ID" > "$ID_FILE"
printf '%s %s\n' "$CONTRACT_ID" "$EXPECTED_HASH" > "$PIN_FILE"
echo "(saved to $ID_FILE)"

echo ""
echo "=== Done ==="
echo "export AGGREGATOR_CONTRACT=$CONTRACT_ID"
echo "Next: deploy escrow with:"
echo "  KEEPER_G=G... AGGREGATOR=$CONTRACT_ID ./contracts/order-escrow/deploy-testnet.sh"

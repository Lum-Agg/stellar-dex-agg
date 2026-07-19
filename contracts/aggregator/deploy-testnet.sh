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
#   AGGREGATOR=C... ./contracts/aggregator/deploy-testnet.sh   # reuse existing id
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

if [[ -n "${AGGREGATOR:-}" ]]; then
  if [[ ! "$AGGREGATOR" =~ ^C[A-Z2-7]{55}$ ]]; then
    echo "ERROR: AGGREGATOR must be a contract id (C…, 56 chars)." >&2
    exit 1
  fi
  echo "=== Reusing existing aggregator (skip deploy) ==="
  echo "AGGREGATOR=$AGGREGATOR"
  echo "$AGGREGATOR" > "$ID_FILE"
  echo "Saved to $ID_FILE"
  exit 0
fi

WASM=$(build_contract_wasm "aggregator-contract" "aggregator_contract" \
  "$ROOT/contracts/aggregator" "$ROOT")

CONTRACT_ID=$(deploy_wasm "$WASM")
echo ""
echo "=== Aggregator deployed ==="
echo "AGGREGATOR=$CONTRACT_ID"
echo "  https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
echo "$CONTRACT_ID" > "$ID_FILE"
echo "(saved to $ID_FILE)"

echo "=== initialize(admin=$ADMIN_G) ==="
run_stellar_tx "initialize" stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account "$ADMIN" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --resource-fee "$INVOKE_RESOURCE_FEE" \
  --inclusion-fee "$INCLUSION_FEE" \
  -- \
  initialize --admin "$ADMIN_G"

echo ""
echo "=== Done ==="
echo "export AGGREGATOR_CONTRACT=$CONTRACT_ID"
echo "Next: deploy escrow with:"
echo "  AGGREGATOR=$CONTRACT_ID ./contracts/order-escrow/deploy-testnet.sh"

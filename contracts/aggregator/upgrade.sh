#!/usr/bin/env bash
# Build optimized WASM and upgrade the deployed aggregator contract on mainnet.
#
# Prerequisites:
#   - stellar CLI (https://developers.stellar.org/docs/tools/cli)
#   - Admin key configured: stellar keys add admin --source-file ...
#   - Network: stellar network use mainnet
#
# Usage:
#   ./contracts/aggregator/upgrade.sh
#   ADMIN=admin AGGREGATOR=CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K ./contracts/aggregator/upgrade.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/contracts/aggregator"

ADMIN="${ADMIN:-admin}"
AGGREGATOR="${AGGREGATOR:-CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K}"
NETWORK="${NETWORK:-mainnet}"
RPC_URL="${RPC_URL:-}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"

echo "=== Building aggregator WASM (release) ==="
stellar contract build --release --optimize 2>/dev/null || {
  echo "stellar contract build --optimize failed, trying cargo + stellar optimize..."
  cargo build -p aggregator-contract --target wasm32v1-none --release
  WASM=""
  for CANDIDATE in \
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

if [[ -n "$RPC_URL" ]]; then
  echo "=== Uploading WASM via RPC_URL ($RPC_URL) ==="
  INSTALL_OUT=$(stellar contract upload \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --source "$ADMIN" \
    --wasm "$WASM")
else
  echo "=== Uploading WASM on network alias '$NETWORK' ==="
  INSTALL_OUT=$(stellar contract upload \
    --network "$NETWORK" \
    --source "$ADMIN" \
    --wasm "$WASM")
fi
echo "$INSTALL_OUT"

# stellar CLI prints hash in various formats; extract 64-char hex
WASM_HASH=$(echo "$INSTALL_OUT" | grep -oE '[0-9a-f]{64}' | tail -1)
if [[ -z "$WASM_HASH" ]]; then
  echo "ERROR: Could not parse WASM hash from install output."
  echo "Install manually, then run:"
  echo "  stellar contract invoke --id $AGGREGATOR --source $ADMIN --network $NETWORK \\"
  echo "    -- upgrade --new_wasm_hash <HASH>"
  exit 1
fi
echo "WASM hash: $WASM_HASH"

echo "=== Upgrading contract $AGGREGATOR ==="
if [[ -n "$RPC_URL" ]]; then
  stellar contract invoke \
    --id "$AGGREGATOR" \
    --source "$ADMIN" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    -- \
    upgrade \
    --new_wasm_hash "$WASM_HASH"
else
  stellar contract invoke \
    --id "$AGGREGATOR" \
    --source "$ADMIN" \
    --network "$NETWORK" \
    -- \
    upgrade \
    --new_wasm_hash "$WASM_HASH"
fi

echo "=== Done ==="
echo "Aggregator upgraded. Verify with a small simulate swap on Sushi/Comet routes."

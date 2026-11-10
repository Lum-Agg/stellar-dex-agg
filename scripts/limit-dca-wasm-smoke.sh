#!/usr/bin/env bash
# Run the compiled contracts in the local Soroban test host, without RPC.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$TARGET"
export CARGO_TARGET_DIR="$(cd "$TARGET" && pwd)"
cd "$ROOT"

cargo build --locked --target wasm32v1-none --profile contract-release \
  -p aggregator-contract -p order-escrow-contract
WASM_DIR="$CARGO_TARGET_DIR/wasm32v1-none/contract-release"
LUMAGG_AGGREGATOR_WASM="$WASM_DIR/aggregator_contract.wasm" \
LUMAGG_ESCROW_WASM="$WASM_DIR/order_escrow_contract.wasm" \
  cargo test --locked -p order-escrow-contract --lib \
    tests::wasm_limit_dca_lifecycle -- --ignored --exact

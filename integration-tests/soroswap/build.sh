#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$here/wasm"
cd "$root"
cargo +1.95.0 build --locked --profile contract-release --target wasm32v1-none -p aggregator-contract
cp "$target_dir/wasm32v1-none/contract-release/aggregator_contract.wasm" "$here/wasm/aggregator_contract.wasm"

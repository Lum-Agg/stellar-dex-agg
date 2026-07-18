#!/usr/bin/env bash
# Independent quote vs on-chain probe (does not affect arb-scanner).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export RPC_URL="${RPC_URL:-http://127.0.0.1:8003}"
export ARB_QUOTE_API_URLS="${ARB_QUOTE_API_URLS:-http://127.0.0.1:3100,http://127.0.0.1:3101,http://127.0.0.1:3102,http://127.0.0.1:3103}"
cd "$ROOT"
RELEASE_BIN="${ROOT}/target/release/quote-sim-probe"
if [[ -x "$RELEASE_BIN" ]]; then
  exec "$RELEASE_BIN" "$@"
fi
cargo run -q -p arbitrage --bin quote-sim-probe -- "$@"

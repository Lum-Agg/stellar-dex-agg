#!/usr/bin/env bash
# Extend a deployed contract instance and its current WASM without adding TTL
# maintenance logic to the contract itself.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  CONTRACT_ID=C... WASM=path/to/current.wasm SOURCE=admin NETWORK=testnet \
    scripts/extend-contract-ttl.sh

Required:
  CONTRACT_ID       Deployed contract instance to extend
  SOURCE            Funded Stellar CLI identity or secret key
  NETWORK           Stellar CLI network name
  WASM or WASM_HASH Exact WASM currently used by the deployed instance

Optional:
  LEDGERS_TO_EXTEND Target remaining TTL in ledgers (default: 2073600, ~120 days)
  PERSISTENT_KEY_XDRS
                      Comma-separated base64 XDR keys to extend for CONTRACT_ID
  DRY_RUN=1         Print commands without submitting transactions
EOF
}

CONTRACT_ID="${CONTRACT_ID:-}"
SOURCE="${SOURCE:-}"
NETWORK="${NETWORK:-}"
WASM="${WASM:-}"
WASM_HASH="${WASM_HASH:-}"
LEDGERS_TO_EXTEND="${LEDGERS_TO_EXTEND:-2073600}"
DRY_RUN="${DRY_RUN:-0}"
PERSISTENT_KEY_XDRS="${PERSISTENT_KEY_XDRS:-}"

if [[ ! "$CONTRACT_ID" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "ERROR: CONTRACT_ID must be a 56-character C... contract ID." >&2
  usage >&2
  exit 1
fi

if [[ -z "$SOURCE" || -z "$NETWORK" ]]; then
  echo "ERROR: SOURCE and NETWORK are required." >&2
  usage >&2
  exit 1
fi

if [[ ! "$LEDGERS_TO_EXTEND" =~ ^[1-9][0-9]*$ ]]; then
  echo "ERROR: LEDGERS_TO_EXTEND must be a positive integer." >&2
  exit 1
fi

if [[ -z "$WASM_HASH" ]]; then
  if [[ -z "$WASM" || ! -f "$WASM" ]]; then
    echo "ERROR: provide WASM_HASH or the exact deployed WASM file." >&2
    usage >&2
    exit 1
  fi
  WASM_HASH=$(openssl dgst -sha256 "$WASM" | awk '{print $NF}')
fi

if [[ ! "$WASM_HASH" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "ERROR: WASM_HASH must be a 64-character hexadecimal SHA-256 hash." >&2
  exit 1
fi

run() {
  printf '+ '
  printf '%q ' "$@"
  printf '\n'
  if [[ "$DRY_RUN" != "1" ]]; then
    "$@"
  fi
}

run stellar contract extend \
  --id "$CONTRACT_ID" \
  --source "$SOURCE" \
  --network "$NETWORK" \
  --ledgers-to-extend "$LEDGERS_TO_EXTEND"

run stellar contract extend \
  --wasm-hash "$WASM_HASH" \
  --source "$SOURCE" \
  --network "$NETWORK" \
  --ledgers-to-extend "$LEDGERS_TO_EXTEND"

if [[ -n "$PERSISTENT_KEY_XDRS" ]]; then
  IFS=',' read -r -a persistent_keys <<< "$PERSISTENT_KEY_XDRS"
  for key_xdr in "${persistent_keys[@]}"; do
    key_xdr="${key_xdr#"${key_xdr%%[![:space:]]*}"}"
    key_xdr="${key_xdr%"${key_xdr##*[![:space:]]}"}"
    [[ -z "$key_xdr" ]] && continue
    run stellar contract extend \
      --id "$CONTRACT_ID" \
      --key-xdr "$key_xdr" \
      --durability persistent \
      --source "$SOURCE" \
      --network "$NETWORK" \
      --ledgers-to-extend "$LEDGERS_TO_EXTEND"
  done
fi

echo "Extended requested TTL targets to at least ${LEDGERS_TO_EXTEND} ledgers."

#!/usr/bin/env bash
# Validate testnet Limit/DCA transaction builders without signing or submitting.
#
# Usage:
#   API=http://127.0.0.1:3200 USER_G=G... ./scripts/limit-dca-testnet-smoke.sh
set -euo pipefail

API="${API:-http://127.0.0.1:3200}"
USER_G="${USER_G:-GBTZVQRXWUTOBJZU5VEZZVNOQIEP7TIHORJFG26FVAHJGCUPDC22BULU}"
XLM="${XLM:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}"
USDC="${USDC:-CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F}"

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

latest="$(curl --fail-with-body -sS "$API/api/v1/ledger/latest")"
ledger="$(jq -er '.sequence' <<<"$latest")"
start_ledger=$((ledger + 10))
expires_ledger=$((ledger + 1000))

build() {
  local label="$1" endpoint="$2" body="$3" response
  response="$(curl --fail-with-body -sS -X POST "$API$endpoint" \
    -H 'content-type: application/json' --data-raw "$body")"
  jq -e '.success == true and (.data.unsigned_tx_xdr | type == "string" and length > 0)' \
    <<<"$response" >/dev/null
  printf 'PASS  %-5s unsigned XDR generated (ledger %s)\n' "$label" "$ledger"
}

build limit /api/v1/orders/build_create \
  "$(jq -cn --arg user "$USER_G" --arg token_in "$XLM" --arg token_out "$USDC" \
    --arg amount_in 500000 --arg limit_out_per_in_e7 6000000 --argjson expires_ledger "$expires_ledger" \
    '{user:$user,token_in:$token_in,token_out:$token_out,amount_in:$amount_in,limit_out_per_in_e7:$limit_out_per_in_e7,expires_ledger:$expires_ledger}')"

build dca /api/v1/dca/build_create \
  "$(jq -cn --arg user "$USER_G" --arg token_in "$XLM" --arg token_out "$USDC" \
    --arg amount_in 500000 --arg chunk_amount 250000 --arg min_out_per_in_e7 0 \
    --argjson interval_ledgers 10 --argjson start_ledger "$start_ledger" \
    --argjson expires_ledger "$expires_ledger" \
    '{user:$user,token_in:$token_in,token_out:$token_out,amount_in:$amount_in,chunk_amount:$chunk_amount,interval_ledgers:$interval_ledgers,start_ledger:$start_ledger,min_out_per_in_e7:$min_out_per_in_e7,expires_ledger:$expires_ledger}')"

# A past start ledger must remain a structured contract error, not a generic trap.
invalid_dca_body="$(jq -cn --arg user "$USER_G" --arg token_in "$XLM" --arg token_out "$USDC" \
  --arg amount_in 500000 --arg chunk_amount 250000 --arg min_out_per_in_e7 0 \
  --argjson interval_ledgers 10 --argjson start_ledger "$((ledger - 1))" \
  --argjson expires_ledger "$expires_ledger" \
  '{user:$user,token_in:$token_in,token_out:$token_out,amount_in:$amount_in,chunk_amount:$chunk_amount,interval_ledgers:$interval_ledgers,start_ledger:$start_ledger,min_out_per_in_e7:$min_out_per_in_e7,expires_ledger:$expires_ledger}')"
invalid_dca_response="$(curl -sS -X POST "$API/api/v1/dca/build_create" \
  -H 'content-type: application/json' --data-raw "$invalid_dca_body")"
jq -e '.success == false and (.error | strings | contains("#17"))' \
  <<<"$invalid_dca_response" >/dev/null
echo "PASS  invalid DCA start ledger returns structured error #17"

echo "No transaction was signed or submitted."

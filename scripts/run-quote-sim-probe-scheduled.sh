#!/usr/bin/env bash
# Scheduled oneshot wrapper for quote-sim-probe (systemd timer / cron).
# Does not affect lumagg-arb. Exit 1 = median gap over threshold or no measurable gaps.
set -euo pipefail

APP_DIR="${APP_DIR:-/opt/stellar-dex-aggregator}"
BIN="${QUOTE_SIM_PROBE_BIN:-${APP_DIR}/target/release/quote-sim-probe}"
LOG_DIR="${QUOTE_SIM_PROBE_LOG_DIR:-${APP_DIR}/logs}"
SAMPLES="${PROBE_SAMPLES:-10}"
THRESHOLD_BPS="${PROBE_THRESHOLD_BPS:-30}"
AMOUNT_IN="${ARB_PROBE_AMOUNT_IN:-100000000}"
SEED="${PROBE_SEED:-$(date +%s)}"

if [[ ! -x "$BIN" ]]; then
  echo "quote-sim-probe binary missing: $BIN" >&2
  exit 1
fi

mkdir -p "$LOG_DIR"
LOG_FILE="${LOG_DIR}/quote-sim-probe.jsonl"

args=(
  --mode round-trip
  --samples "$SAMPLES"
  --seed "$SEED"
  --amount-in "$AMOUNT_IN"
  --threshold-bps "$THRESHOLD_BPS"
  --jsonl
)
if [[ "${PROBE_SIMULATE:-1}" == "1" ]]; then
  args+=(--simulate)
fi

echo "=== quote-sim-probe start seed=${SEED} samples=${SAMPLES} threshold_bps=${THRESHOLD_BPS} simulate=${PROBE_SIMULATE:-1} ===" >&2

set +e
"$BIN" "${args[@]}" | tee -a "$LOG_FILE"
rc=$?
set -e

echo "=== quote-sim-probe done exit=${rc} ===" >&2
exit "$rc"

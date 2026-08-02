#!/bin/bash
# Deploy LumAgg arbitrage bot (vault mode) to production server.
#
# Prerequisites on server:
#   - stellar-rpc + redis + lumagg-worker (snapshot publisher)
#   - /etc/mnemonic_code.txt with caller mnemonic (indices 1–9 funded for fees)
#   - deploy/arb.env with ARB_SUBMIT_TX=0 for dry-run, then 1 for live submit
#
# Usage:
#   ./deploy_arb.sh              # build, install unit, restart service
#   ./deploy_arb.sh install      # install binary + unit only (no restart)
#   START=0 ./deploy_arb.sh        # build + install, skip systemctl restart
#
# Local overrides (optional, not committed):
#   cp scripts/arb.env.example scripts/arb.env.local
#   edit scripts/arb.env.local  →  copied to server deploy/arb.env
set -euo pipefail

MODE="${1:-deploy}"
case "$MODE" in
  deploy | install) ;;
  *)
    echo "Usage: $0 [deploy|install]" >&2
    exit 1
    ;;
esac

SERVER="root@88.198.16.144"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_ARB_BIN="${REMOTE_APP_DIR}/target/release/lumagg-arbitrage-bot"
START="${START:-1}"

echo "=== LumAgg arbitrage bot deploy (mode=${MODE}, start=${START}) ==="

echo "=== Syncing source code ==="
rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude node_modules \
  --exclude thirdparty \
  --exclude out \
  --exclude packages/frontend \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$(dirname "$0")/" \
  "${SERVER}:${REMOTE_SRC}/"

echo "=== Building lumagg-arbitrage-bot + quote-sim-probe on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p arbitrage --bin lumagg-arbitrage-bot --bin quote-sim-probe --bin diag_simulate 2>&1 | tail -20"

echo "=== Arb env (server-only) ==="
ARB_ENV_FILE="$(dirname "$0")/scripts/arb.env.local"
if [[ -f "$ARB_ENV_FILE" ]]; then
  ssh -o StrictHostKeyChecking=no "$SERVER" "mkdir -p ${REMOTE_APP_DIR}/deploy"
  scp -o StrictHostKeyChecking=no "$ARB_ENV_FILE" "${SERVER}:${REMOTE_APP_DIR}/deploy/arb.env"
  ssh -o StrictHostKeyChecking=no "$SERVER" "chmod 600 ${REMOTE_APP_DIR}/deploy/arb.env"
else
  echo "WARN: ${ARB_ENV_FILE} missing — create from scripts/arb.env.example before live submit"
  echo "      Service defaults to ARB_SUBMIT_TX=0 (simulate-only) from lumagg-arb.service"
fi

echo "=== Installing binary + systemd unit + quote-sim-probe timer ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' START='${START}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_ARB_BIN='${REMOTE_ARB_BIN}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/scripts" "${REMOTE_APP_DIR}/logs"
deploy_arb() {
  systemctl stop lumagg-arb >/dev/null 2>&1 || true
  cp "${REMOTE_SRC}/target/release/lumagg-arbitrage-bot" "${REMOTE_ARB_BIN}"
  cp "${REMOTE_SRC}/target/release/quote-sim-probe" "${REMOTE_APP_DIR}/target/release/quote-sim-probe"
  chmod +x "${REMOTE_APP_DIR}/target/release/quote-sim-probe"
  cp "${REMOTE_SRC}/scripts/run-quote-sim-probe-scheduled.sh" "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  chmod +x "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  cp "${REMOTE_SRC}/deploy/lumagg-arb.service" /etc/systemd/system/lumagg-arb.service
  cp "${REMOTE_SRC}/deploy/lumagg-quote-sim-probe.service" /etc/systemd/system/lumagg-quote-sim-probe.service
  cp "${REMOTE_SRC}/deploy/lumagg-quote-sim-probe.timer" /etc/systemd/system/lumagg-quote-sim-probe.timer
  systemctl daemon-reload
  systemctl enable lumagg-arb
  systemctl enable --now lumagg-quote-sim-probe.timer

  if [[ "${MODE}" == "install" || "${START}" == "0" ]]; then
    echo "Skipping systemctl start (MODE=${MODE} START=${START})"
    exit 0
  fi

  # Stop legacy stellar-arb if still running (best-effort)
  systemctl disable --now stellar-arb >/dev/null 2>&1 || true

  systemctl restart lumagg-arb
}

deploy_arb
REMOTE

if [[ "$MODE" == "deploy" && "$START" == "1" ]]; then
  echo "=== Arb bot status ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "systemctl status lumagg-arb --no-pager | head -14; echo; journalctl -u lumagg-arb -n 20 --no-pager"
  echo "=== Quote-sim-probe timer ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "systemctl status lumagg-quote-sim-probe.timer --no-pager | head -14; systemctl list-timers lumagg-quote-sim-probe.timer --no-pager"
fi

echo "=== Done ==="
echo "Vault:  CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN"
echo "Dry-run: ARB_SUBMIT_TX=0 in service/arb.env — set ARB_SUBMIT_TX=1 in deploy/arb.env then: systemctl restart lumagg-arb"
echo "Probe timer: systemctl status lumagg-quote-sim-probe.timer  (every 30m; logs: journalctl -u lumagg-quote-sim-probe)"

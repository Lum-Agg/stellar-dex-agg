#!/bin/bash
# Deploy LumAgg arbitrage bot (vault mode) to production server.
#
# Prerequisites on server:
#   - stellar-rpc + redis + lumagg-worker (snapshot publisher)
#   - /etc/mnemonic_code.txt with caller mnemonic (indices 1–9 funded for fees)
#   - configs/production-arbitrage.toml generated and reviewed
#
# Usage:
#   ./ops/deploy_arb.sh              # build, install unit, restart service
#   ./ops/deploy_arb.sh install      # install binary + unit only (no restart)
#   START=0 ./ops/deploy_arb.sh      # build + install, skip systemctl restart
#
# Private config is not committed. Start from packaging/lumagg-arbitrage.toml
# when configuring a new machine.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

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
PRODUCTION_CONFIG="$ROOT/configs/production-arbitrage.toml"

if [[ ! -f "$PRODUCTION_CONFIG" ]]; then
  echo "ERROR: Missing ${PRODUCTION_CONFIG}; create it from packaging/lumagg-arbitrage.toml" >&2
  exit 1
fi

echo "=== LumAgg arbitrage bot deploy (mode=${MODE}, start=${START}) ==="

echo "=== Syncing source code ==="
rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude node_modules \
  --exclude thirdparty \
  --exclude out \
  --exclude packages/frontend \
  --exclude configs \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$ROOT/" \
  "${SERVER}:${REMOTE_SRC}/"

echo "=== Building lumagg-arbitrage-bot + quote-sim-probe on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p arbitrage --bin lumagg-arbitrage-bot --bin quote-sim-probe --bin diag_simulate 2>&1 | tail -20"

echo "=== Uploading private Arbitrage TOML ==="
ssh -o StrictHostKeyChecking=no "$SERVER" "mkdir -p ${REMOTE_APP_DIR}/deploy"
scp -o StrictHostKeyChecking=no "$PRODUCTION_CONFIG" "${SERVER}:${REMOTE_APP_DIR}/deploy/arbitrage.toml"
ssh -o StrictHostKeyChecking=no "$SERVER" "chmod 600 ${REMOTE_APP_DIR}/deploy/arbitrage.toml"

echo "=== Installing binary + systemd unit + quote-sim-probe timer ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' START='${START}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_ARB_BIN='${REMOTE_ARB_BIN}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/scripts" "${REMOTE_APP_DIR}/logs"
deploy_arb() {
  "${REMOTE_SRC}/target/release/lumagg-arbitrage-bot" \
    --config "${REMOTE_APP_DIR}/deploy/arbitrage.toml" --check-config
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
echo "Submission mode: edit configs/production-arbitrage.toml, redeploy, then check journalctl -u lumagg-arb"
echo "Probe timer: systemctl status lumagg-quote-sim-probe.timer  (every 30m; logs: journalctl -u lumagg-quote-sim-probe)"

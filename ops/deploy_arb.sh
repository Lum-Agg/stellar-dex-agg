#!/bin/bash
# Deploy LumAgg arbitrage bot (vault mode) to production server.
#
# Prerequisites on server:
#   - stellar-rpc + redis + lumagg-worker (snapshot publisher)
#   - /etc/mnemonic_code.txt with caller mnemonic (indices 1–9 funded for fees)
#   - Existing systemd unit and reviewed deploy/arbitrage.toml on the server
#
# Usage:
#   ./ops/deploy_arb.sh              # build and install, restart existing service
#   ./ops/deploy_arb.sh install      # install binaries only (no restart)
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

SERVER="${LUMAGG_DEPLOY_SERVER:-root@162.245.187.234}"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_ARB_BIN="${REMOTE_APP_DIR}/target/release/lumagg-arbitrage-bot"
START="${START:-1}"
echo "=== LumAgg arbitrage bot deploy on ${SERVER} (mode=${MODE}, start=${START}) ==="
ssh -o BatchMode=yes "$SERVER" \
  "test -r '${REMOTE_APP_DIR}/deploy/arbitrage.toml' && test -r /etc/systemd/system/lumagg-arb.service"

echo "=== Syncing source code ==="
rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude node_modules \
  --exclude thirdparty \
  --exclude out \
  --exclude packages/frontend \
  --exclude configs \
  --exclude /data/ \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$ROOT/" \
  "${SERVER}:${REMOTE_SRC}/"

echo "=== Building lumagg-arbitrage-bot + quote-sim-probe on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "bash -o pipefail -c 'source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p arbitrage --bin lumagg-arbitrage-bot --bin quote-sim-probe --bin diag_simulate 2>&1 | tail -20'"

echo "=== Installing binaries; retaining server configuration and units ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' START='${START}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_ARB_BIN='${REMOTE_ARB_BIN}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/scripts" "${REMOTE_APP_DIR}/logs"
deploy_arb() {
  "${REMOTE_SRC}/target/release/lumagg-arbitrage-bot" \
    --config "${REMOTE_APP_DIR}/deploy/arbitrage.toml" --check-config
  replace_binary() {
    local staging
    staging=$(mktemp "$2.XXXXXX")
    install -m 0755 "$1" "$staging"
    mv -f "$staging" "$2"
  }
  replace_binary "${REMOTE_SRC}/target/release/lumagg-arbitrage-bot" "${REMOTE_ARB_BIN}"
  replace_binary "${REMOTE_SRC}/target/release/quote-sim-probe" "${REMOTE_APP_DIR}/target/release/quote-sim-probe"
  cp "${REMOTE_SRC}/scripts/run-quote-sim-probe-scheduled.sh" "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  chmod +x "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  if [[ "${MODE}" == "install" || "${START}" == "0" ]]; then
    echo "Skipping systemctl start (MODE=${MODE} START=${START})"
    exit 0
  fi

  # Stop legacy stellar-arb if still running (best-effort)
  systemctl disable --now stellar-arb >/dev/null 2>&1 || true

  systemctl restart lumagg-arb
  systemctl is-active --quiet lumagg-arb
  systemctl enable lumagg-arb
}

deploy_arb
REMOTE

if [[ "$MODE" == "deploy" && "$START" == "1" ]]; then
  echo "=== Arb bot status ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "systemctl status lumagg-arb --no-pager | head -14; echo; journalctl -u lumagg-arb -n 20 --no-pager"
fi

echo "=== Done ==="
echo "Vault:  CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN"
echo "Submission mode: managed in the server's deploy/arbitrage.toml"

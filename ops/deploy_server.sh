#!/bin/bash
# Deploy LumAgg backend (Redis snapshot / pool-state architecture).
#
# Usage:
#   ./ops/deploy_server.sh          # same as "all"
#   ./ops/deploy_server.sh all      # api-server + market-data-worker
#   ./ops/deploy_server.sh api      # api-server only (4 instances, ports 3100-3103)
#   ./ops/deploy_server.sh worker   # market-data-worker only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

MODE="${1:-all}"
case "$MODE" in
  all | api | worker) ;;
  *)
    echo "Usage: $0 [all|api|worker]" >&2
    exit 1
    ;;
esac

SERVER="root@88.198.16.144"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_API_BIN="${REMOTE_APP_DIR}/target/release/lumagg-api-server"
REMOTE_WORKER_BIN="${REMOTE_APP_DIR}/target/release/lumagg-market-data-worker"
API_PORTS=(3100 3101 3102 3103)
API_PORTS_STR="${API_PORTS[*]}"
PRIMARY_PORT="${PRIMARY_PORT:-3100}"
REMOTE_API_BASE="http://127.0.0.1:${PRIMARY_PORT}"
# Must match deploy/lumagg-*.service (used only for post-deploy verify on server)
REDIS_URL="${REDIS_URL:-redis://:REDISzlg153@127.0.0.1:6379/}"
PRODUCTION_CONFIG="$ROOT/configs/production-aggregator.toml"

if [[ ! -f "$PRODUCTION_CONFIG" ]]; then
  echo "ERROR: Missing ${PRODUCTION_CONFIG}; create it from packaging/lumagg-aggregator.toml" >&2
  exit 1
fi

echo "=== LumAgg deploy mode: ${MODE} ==="

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

BUILD_PKGS=()
if [[ "$MODE" == "all" || "$MODE" == "api" ]]; then
  BUILD_PKGS+=(-p api-server)
fi
if [[ "$MODE" == "all" || "$MODE" == "worker" ]]; then
  BUILD_PKGS+=(-p market-data-worker)
fi

echo "=== Building on server (${BUILD_PKGS[*]}) ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release ${BUILD_PKGS[*]} 2>&1 | tail -12"

echo "=== Uploading private Aggregator TOML ==="
ssh -o StrictHostKeyChecking=no "$SERVER" "mkdir -p ${REMOTE_APP_DIR}/deploy"
scp -o StrictHostKeyChecking=no "$PRODUCTION_CONFIG" "${SERVER}:${REMOTE_APP_DIR}/deploy/aggregator.toml"
ssh -o StrictHostKeyChecking=no "$SERVER" "chmod 600 ${REMOTE_APP_DIR}/deploy/aggregator.toml"

echo "=== Deploying binaries + systemd units (mode=${MODE}) ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_API_BIN='${REMOTE_API_BIN}' REMOTE_WORKER_BIN='${REMOTE_WORKER_BIN}' API_PORTS_STR='${API_PORTS_STR}' bash -s" <<'REMOTE'
set -euo pipefail

mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/data/logos"
"${REMOTE_SRC}/target/release/lumagg-market-data-worker" \
  --config "${REMOTE_APP_DIR}/deploy/aggregator.toml" --check-config
"${REMOTE_SRC}/target/release/lumagg-api-server" \
  --config "${REMOTE_APP_DIR}/deploy/aggregator.toml" --check-config
cp "${REMOTE_SRC}/deploy/lumagg-api@.service" /etc/systemd/system/lumagg-api@.service
cp "${REMOTE_SRC}/deploy/lumagg-worker.service" /etc/systemd/system/lumagg-worker.service
rm -f /etc/systemd/system/lumagg-api.service
systemctl disable --now lumagg-api >/dev/null 2>&1 || true

deploy_worker() {
  # Unlink first — overwriting a running/mapped ELF hits "Text file busy".
  rm -f "${REMOTE_WORKER_BIN}"
  cp "${REMOTE_SRC}/target/release/lumagg-market-data-worker" "${REMOTE_WORKER_BIN}"
  systemctl enable lumagg-worker
  systemctl restart lumagg-worker
}

deploy_api() {
  # Unlink first — overwriting a running/mapped ELF hits "Text file busy".
  rm -f "${REMOTE_API_BIN}"
  cp "${REMOTE_SRC}/target/release/lumagg-api-server" "${REMOTE_API_BIN}"
  for port in ${API_PORTS_STR}; do
    systemctl enable "lumagg-api@${port}"
    systemctl restart "lumagg-api@${port}"
  done
}

systemctl daemon-reload

case "${MODE}" in
  all)
    deploy_worker
    deploy_api
    ;;
  api)
    deploy_api
    ;;
  worker)
    deploy_worker
    ;;
esac
REMOTE

if [[ "$MODE" == "all" || "$MODE" == "worker" ]]; then
  echo "=== Waiting for worker pool publish (verify script polls Redis) ==="
  echo "=== Worker logs (last 15 lines) ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" "journalctl -u lumagg-worker -n 15 --no-pager" || true
fi

if [[ "$MODE" == "api" ]]; then
  echo "=== API logs (last 10 lines, lumagg-api@${PRIMARY_PORT}) ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "journalctl -u lumagg-api@${PRIMARY_PORT} -n 10 --no-pager" || true
fi

echo "=== Stack verify (health, Redis keys, quote) ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "API_BASE=${REMOTE_API_BASE} REDIS_URL='${REDIS_URL}' bash -s" \
  < "$ROOT/scripts/verify_redis_stack.sh"

echo "=== Done (mode=${MODE}) ==="

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

SERVER="${LUMAGG_DEPLOY_SERVER:-root@162.245.187.234}"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_API_BIN="${REMOTE_APP_DIR}/target/release/lumagg-api-server"
REMOTE_WORKER_BIN="${REMOTE_APP_DIR}/target/release/lumagg-market-data-worker"
API_PORTS=(3100 3101 3102 3103)
API_PORTS_STR="${API_PORTS[*]}"
PRIMARY_PORT="${PRIMARY_PORT:-3100}"
REMOTE_API_BASE="http://127.0.0.1:${PRIMARY_PORT}"
REMOTE_CONFIG="${REMOTE_APP_DIR}/deploy/aggregator.toml"

echo "=== LumAgg deploy mode: ${MODE} on ${SERVER} ==="
ssh -o BatchMode=yes "$SERVER" \
  "test -r '${REMOTE_CONFIG}' && test -r /etc/systemd/system/lumagg-api@.service && test -r /etc/systemd/system/lumagg-worker.service"

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

BUILD_PKGS=()
if [[ "$MODE" == "all" || "$MODE" == "api" ]]; then
  BUILD_PKGS+=(-p api-server)
fi
if [[ "$MODE" == "all" || "$MODE" == "worker" ]]; then
  BUILD_PKGS+=(-p market-data-worker)
fi

echo "=== Building on server (${BUILD_PKGS[*]}) ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "bash -o pipefail -c 'source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release ${BUILD_PKGS[*]} 2>&1 | tail -12'"

echo "=== Deploying binaries (mode=${MODE}); retaining server configuration and units ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_API_BIN='${REMOTE_API_BIN}' REMOTE_WORKER_BIN='${REMOTE_WORKER_BIN}' API_PORTS_STR='${API_PORTS_STR}' bash -s" <<'REMOTE'
set -euo pipefail

mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/data/logos"
if [[ "$MODE" == all || "$MODE" == worker ]]; then
  "${REMOTE_SRC}/target/release/lumagg-market-data-worker" \
    --config "${REMOTE_APP_DIR}/deploy/aggregator.toml" --check-config
fi
if [[ "$MODE" == all || "$MODE" == api ]]; then
  "${REMOTE_SRC}/target/release/lumagg-api-server" \
    --config "${REMOTE_APP_DIR}/deploy/aggregator.toml" --check-config
fi

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
  "API_BASE='${REMOTE_API_BASE}' AGGREGATOR_CONFIG='${REMOTE_CONFIG}' bash -s" \
  < "$ROOT/scripts/verify_redis_stack.sh"

echo "=== Done (mode=${MODE}) ==="

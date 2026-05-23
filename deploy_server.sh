#!/bin/bash
# Deploy API + market-data-worker (Redis snapshot / pool-state architecture)
# Usage: ./deploy_server.sh
set -e

SERVER="root@178.63.81.216"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_API_BIN="${REMOTE_APP_DIR}/target/release/api-server"
REMOTE_WORKER_BIN="${REMOTE_APP_DIR}/target/release/market-data-worker"
API_PORTS=(3100 3101 3102 3103)
PRIMARY_PORT="${PRIMARY_PORT:-3100}"
REMOTE_API_BASE="http://127.0.0.1:${PRIMARY_PORT}"
# Must match deploy/lumagg-*.service (used only for post-deploy verify on server)
REDIS_URL="${REDIS_URL:-redis://:REDISzlg153@127.0.0.1:6379/}"

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

echo "=== Building on server (api-server + market-data-worker) ==="
ssh -o StrictHostKeyChecking=no $SERVER "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p api-server -p market-data-worker 2>&1 | tail -8"

echo "=== Deploying binaries + systemd units ==="
ssh -o StrictHostKeyChecking=no $SERVER "\
  set -euo pipefail; \
  mkdir -p ${REMOTE_APP_DIR}/target/release; \
  systemctl disable --now lumagg-api >/dev/null 2>&1 || true; \
  for port in ${API_PORTS[*]}; do systemctl stop lumagg-api@\$port >/dev/null 2>&1 || true; done; \
  systemctl stop lumagg-worker >/dev/null 2>&1 || true; \
  cp ${REMOTE_SRC}/target/release/api-server ${REMOTE_API_BIN}; \
  cp ${REMOTE_SRC}/target/release/market-data-worker ${REMOTE_WORKER_BIN}; \
  cp ${REMOTE_SRC}/deploy/lumagg-api@.service /etc/systemd/system/lumagg-api@.service; \
  cp ${REMOTE_SRC}/deploy/lumagg-worker.service /etc/systemd/system/lumagg-worker.service; \
  rm -f /etc/systemd/system/lumagg-api.service; \
  systemctl daemon-reload; \
  systemctl enable --now lumagg-worker; \
  for port in ${API_PORTS[*]}; do systemctl enable --now lumagg-api@\$port; done \
"

echo "=== Waiting for worker snapshot + pool publish (first cycle) ==="
sleep 15

echo "=== Worker logs (last 15 lines) ==="
ssh -o StrictHostKeyChecking=no $SERVER "journalctl -u lumagg-worker -n 15 --no-pager" || true

echo "=== Stack verify (health, Redis keys, quote) ==="
ssh -o StrictHostKeyChecking=no $SERVER \
  "API_BASE=${REMOTE_API_BASE} REDIS_URL='${REDIS_URL}' bash -s" \
  < "$(dirname "$0")/scripts/verify_redis_stack.sh"

echo "=== Done ==="

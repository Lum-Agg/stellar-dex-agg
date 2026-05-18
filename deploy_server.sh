#!/bin/bash
# Deploy API server to 178.63.81.216
# Usage: ./deploy_server.sh
set -e

SERVER="root@178.63.81.216"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_BIN="/opt/stellar-dex-aggregator/target/release/api-server"

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

echo "=== Building on server ==="
ssh -o StrictHostKeyChecking=no $SERVER "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p api-server 2>&1 | tail -3"

echo "=== Deploying ==="
ssh -o StrictHostKeyChecking=no $SERVER "systemctl stop lumagg-api; cp ${REMOTE_SRC}/target/release/api-server ${REMOTE_BIN}; systemctl start lumagg-api"

echo "=== Waiting for startup ==="
sleep 5

echo "=== Testing ==="
curl -s "https://api.lumagg.xyz/api/v1/health" && echo ""
echo "=== Done ==="

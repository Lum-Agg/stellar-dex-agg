#!/bin/bash
# Deploy API server to 178.63.81.216
# Usage: ./deploy_server.sh
set -e

SERVER="root@178.63.81.216"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
REMOTE_BIN="${REMOTE_APP_DIR}/target/release/api-server"
API_PORTS=(3100 3101 3102 3103)
PRIMARY_PORT="${PRIMARY_PORT:-3100}"
REMOTE_API_BASE="http://127.0.0.1:${PRIMARY_PORT}"

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
ssh -o StrictHostKeyChecking=no $SERVER "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p api-server --bin api-server 2>&1 | tail -5"

echo "=== Deploying ==="
ssh -o StrictHostKeyChecking=no $SERVER "\
  set -euo pipefail; \
  systemctl disable --now lumagg-api >/dev/null 2>&1 || true; \
  for port in ${API_PORTS[*]}; do systemctl stop lumagg-api@\$port >/dev/null 2>&1 || true; done; \
  cp ${REMOTE_SRC}/target/release/api-server ${REMOTE_BIN}; \
  cp ${REMOTE_SRC}/deploy/lumagg-api@.service /etc/systemd/system/lumagg-api@.service; \
  rm -f /etc/systemd/system/lumagg-api.service; \
  systemctl daemon-reload; \
  for port in ${API_PORTS[*]}; do systemctl enable --now lumagg-api@\$port; done \
"

echo "=== Waiting for startup (pool cache load) ==="
sleep 8

echo "=== Health ==="
ssh -o StrictHostKeyChecking=no $SERVER "curl -sf ${REMOTE_API_BASE}/api/v1/health" && echo ""

echo "=== Quote smoke test (1 XLM -> USDC, 30s max) ==="
QUOTE_URL="${REMOTE_API_BASE}/api/v1/quote?token_in=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA&token_out=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75&amount_in=10000000&slippage=0.5"
if ssh -o StrictHostKeyChecking=no $SERVER "curl -sf --max-time 30 '$QUOTE_URL'" | head -c 500; then
  echo ""
  echo "Quote OK"
else
  echo ""
  echo "WARN: quote failed or timed out — check: journalctl -u lumagg-api@${PRIMARY_PORT} -n 50"
  exit 1
fi

echo "=== Done ==="

#!/bin/bash
# Deploy API server to 178.63.81.216
# Usage: ./deploy_server.sh
set -e

SERVER="root@178.63.81.216"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_BIN="/opt/stellar-dex-aggregator/target/release/api-server"
API_BASE="${API_BASE:-https://api.lumagg.xyz}"

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
ssh -o StrictHostKeyChecking=no $SERVER "systemctl stop lumagg-api; cp ${REMOTE_SRC}/target/release/api-server ${REMOTE_BIN}; systemctl start lumagg-api"

echo "=== Waiting for startup (pool cache load) ==="
sleep 8

echo "=== Health ==="
curl -sf "${API_BASE}/api/v1/health" && echo ""

echo "=== Quote smoke test (1 XLM -> USDC, 30s max) ==="
QUOTE_URL="${API_BASE}/api/v1/quote?token_in=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA&token_out=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75&amount_in=10000000&slippage=0.5"
if curl -sf --max-time 30 "$QUOTE_URL" | head -c 500; then
  echo ""
  echo "Quote OK"
else
  echo ""
  echo "WARN: quote failed or timed out — check: journalctl -u lumagg-api -n 50"
  exit 1
fi

echo "=== Done ==="

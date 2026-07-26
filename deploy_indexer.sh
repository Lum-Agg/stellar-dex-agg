#!/bin/bash
# Deploy LumAgg analytics-indexer to production server.
#
# Usage:
#   ./deploy_indexer.sh
#   INDEXER_START_LEDGER=63200000 ./deploy_indexer.sh   # optional backfill after deploy
set -euo pipefail

SERVER="root@88.198.16.144"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
INDEXER_START_LEDGER="${INDEXER_START_LEDGER:-}"

echo "=== LumAgg analytics-indexer deploy ==="

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

echo "=== Building analytics-indexer on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p analytics-indexer 2>&1 | tail -12"

echo "=== Installing binary + systemd unit ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/data" "${REMOTE_APP_DIR}/deploy"
cp "${REMOTE_SRC}/target/release/analytics-indexer" "${REMOTE_APP_DIR}/target/release/analytics-indexer"
cp "${REMOTE_SRC}/deploy/lumagg-indexer.service" /etc/systemd/system/lumagg-indexer.service
systemctl daemon-reload
systemctl enable lumagg-indexer
systemctl restart lumagg-indexer
REMOTE

echo "=== Indexer status ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "systemctl status lumagg-indexer --no-pager | head -12; echo; journalctl -u lumagg-indexer -n 15 --no-pager"

if [[ -n "$INDEXER_START_LEDGER" ]]; then
  echo "=== One-shot backfill from ledger ${INDEXER_START_LEDGER} ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "cd ${REMOTE_APP_DIR} && INDEXER_START_LEDGER=${INDEXER_START_LEDGER} ${REMOTE_APP_DIR}/target/release/analytics-indexer backfill"
fi

echo "=== Done ==="

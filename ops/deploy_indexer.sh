#!/bin/bash
# Deploy LumAgg analytics-indexer to production server.
#
# Usage:
#   ./ops/deploy_indexer.sh
#   INDEXER_START_LEDGER=63200000 ./ops/deploy_indexer.sh   # optional backfill after deploy
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

SERVER="${LUMAGG_DEPLOY_SERVER:-root@162.245.187.234}"
REMOTE_SRC="/opt/stellar-dex-aggregator-src"
REMOTE_APP_DIR="/opt/stellar-dex-aggregator"
INDEXER_START_LEDGER="${INDEXER_START_LEDGER:-}"
echo "=== LumAgg analytics-indexer deploy on ${SERVER} ==="
ssh -o BatchMode=yes "$SERVER" \
  "test -r '${REMOTE_APP_DIR}/deploy/aggregator.toml' && test -r /etc/systemd/system/lumagg-indexer.service"

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

echo "=== Building analytics-indexer on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "bash -o pipefail -c 'source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p analytics-indexer 2>&1 | tail -12'"

echo "=== Installing binary; retaining server configuration and unit ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/data" "${REMOTE_APP_DIR}/deploy"
"${REMOTE_SRC}/target/release/lumagg-analytics-indexer" \
  --config "${REMOTE_APP_DIR}/deploy/aggregator.toml" --check-config
db="${REMOTE_APP_DIR}/data/analytics-indexer.db"
if [[ -f "$db" ]]; then
  backup_dir="${REMOTE_APP_DIR}/data/backups"
  backup="${backup_dir}/analytics-indexer-$(date -u +%Y%m%dT%H%M%SZ).db"
  mkdir -p "$backup_dir"
  sqlite3 "$db" ".backup '$backup'"
  find "$backup_dir" -name 'analytics-indexer-*.db' -type f -printf '%T@ %p\n' \
    | sort -nr | tail -n +6 | cut -d' ' -f2- | xargs -r rm -f
fi
rm -f "${REMOTE_APP_DIR}/target/release/lumagg-analytics-indexer"
cp "${REMOTE_SRC}/target/release/lumagg-analytics-indexer" \
  "${REMOTE_APP_DIR}/target/release/lumagg-analytics-indexer"
systemctl enable lumagg-indexer
systemctl restart lumagg-indexer
REMOTE

echo "=== Indexer status ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "systemctl status lumagg-indexer --no-pager | head -12; echo; journalctl -u lumagg-indexer -n 15 --no-pager; echo; cd ${REMOTE_APP_DIR} && ${REMOTE_APP_DIR}/target/release/lumagg-analytics-indexer --config ${REMOTE_APP_DIR}/deploy/aggregator.toml status"

if [[ -n "$INDEXER_START_LEDGER" ]]; then
  echo "=== One-shot backfill from ledger ${INDEXER_START_LEDGER} ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "cd ${REMOTE_APP_DIR} && ${REMOTE_APP_DIR}/target/release/lumagg-analytics-indexer --config ${REMOTE_APP_DIR}/deploy/aggregator.toml backfill --start-ledger ${INDEXER_START_LEDGER}"
fi

echo "=== Done ==="

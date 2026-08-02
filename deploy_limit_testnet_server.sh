#!/bin/bash
# Deploy the isolated Limit/DCA testnet stack without replacing mainnet binaries.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
SERVER="${SERVER:-root@88.198.16.144}"
REMOTE_SRC="${REMOTE_SRC:-/opt/stellar-dex-aggregator-src}"
REMOTE_APP_DIR="${REMOTE_APP_DIR:-/opt/stellar-dex-aggregator}"
ENV_FILE="${ENV_FILE:-$ROOT/deploy/.env.limit-testnet.local}"
RESET_TESTNET_DB="${RESET_TESTNET_DB:-0}"
INDEXER_CONFIG="$(mktemp)"
trap 'rm -f "$INDEXER_CONFIG"' EXIT

if [[ ! -f "$ENV_FILE" ]]; then
  echo "ERROR: Missing $ENV_FILE. Run scripts/deploy-limit-testnet.sh first." >&2
  exit 1
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

if [[ "${KEEPER_NETWORK:-}" != "testnet" ]]; then
  echo "ERROR: KEEPER_NETWORK must be testnet." >&2
  exit 1
fi
if [[ "${NETWORK_PASSPHRASE:-}" == *"Public Global Stellar Network"* ]]; then
  echo "ERROR: Refusing mainnet passphrase." >&2
  exit 1
fi
if [[ ! "${AGGREGATOR_CONTRACT:-}" =~ ^C[A-Z2-7]{55}$ ]] ||
  [[ ! "${ESCROW_CONTRACT:-}" =~ ^C[A-Z2-7]{55}$ ]]
then
  echo "ERROR: Invalid testnet contract IDs in $ENV_FILE." >&2
  exit 1
fi

INDEXER_RPC_URL="${INDEXER_RPC_URL:-${SOROBAN_RPC_URL:-${RPC_URL:-}}}"
if [[ -z "$INDEXER_RPC_URL" ]]; then
  echo "ERROR: Missing INDEXER_RPC_URL, SOROBAN_RPC_URL, or RPC_URL in $ENV_FILE." >&2
  exit 1
fi

cat >"$INDEXER_CONFIG" <<EOF
[network]
rpc_url = "$INDEXER_RPC_URL"
passphrase = "$NETWORK_PASSPHRASE"

[api]
aggregator_contract = "$AGGREGATOR_CONTRACT"

[features]
escrow_contract = "$ESCROW_CONTRACT"

[indexer]
db_path = "$REMOTE_APP_DIR/data/analytics-indexer-testnet.db"
mode = "events"
envelope_fallback = false
poll_secs = 15
page_limit = 10000

[monitoring]
log_filter = "info"
EOF
chmod 600 "$INDEXER_CONFIG"

echo "=== Sync source to $SERVER ==="
rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude node_modules \
  --exclude thirdparty \
  --exclude out \
  --exclude packages/frontend \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$ROOT/" "${SERVER}:${REMOTE_SRC}/"

echo "=== Build isolated testnet binaries ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd '$REMOTE_SRC' && cargo build --release \
    -p api-server -p analytics-indexer -p limit-keeper -p market-data-worker"

echo "=== Install testnet stack ==="
scp -o StrictHostKeyChecking=no "$ENV_FILE" \
  "${SERVER}:${REMOTE_APP_DIR}/deploy/.env.limit-testnet.local"
scp -o StrictHostKeyChecking=no "$INDEXER_CONFIG" \
  "${SERVER}:${REMOTE_APP_DIR}/deploy/indexer-testnet.toml"

ssh -o StrictHostKeyChecking=no "$SERVER" \
  "REMOTE_SRC='$REMOTE_SRC' REMOTE_APP_DIR='$REMOTE_APP_DIR' RESET_TESTNET_DB='$RESET_TESTNET_DB' bash -s" <<'REMOTE'
set -euo pipefail

if [[ ! -f /etc/lumagg/testnet-redis.env ]]; then
  echo "ERROR: Missing /etc/lumagg/testnet-redis.env" >&2
  exit 1
fi

systemctl stop \
  lumagg-limit-keeper-testnet \
  lumagg-api-testnet \
  lumagg-indexer-testnet \
  lumagg-worker-testnet 2>/dev/null || true
mkdir -p "$REMOTE_APP_DIR/target/release" "$REMOTE_APP_DIR/data" "$REMOTE_APP_DIR/deploy"
chmod 600 "$REMOTE_APP_DIR/deploy/.env.limit-testnet.local"
chmod 600 "$REMOTE_APP_DIR/deploy/indexer-testnet.toml"

if [[ "$RESET_TESTNET_DB" == "1" ]]; then
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  db="$REMOTE_APP_DIR/data/analytics-indexer-testnet.db"
  cursor="$REMOTE_APP_DIR/data/limit-keeper-testnet.cursor"
  [[ ! -f "$db" ]] || mv "$db" "${db}.bak.${stamp}"
  [[ ! -f "$cursor" ]] || mv "$cursor" "${cursor}.bak.${stamp}"
fi

install -m 755 "$REMOTE_SRC/target/release/lumagg-api-server" \
  "$REMOTE_APP_DIR/target/release/lumagg-api-server-testnet"
install -m 755 "$REMOTE_SRC/target/release/lumagg-analytics-indexer" \
  "$REMOTE_APP_DIR/target/release/lumagg-analytics-indexer-testnet"
install -m 755 "$REMOTE_SRC/target/release/limit-keeper" \
  "$REMOTE_APP_DIR/target/release/limit-keeper-testnet"
install -m 755 "$REMOTE_SRC/target/release/lumagg-market-data-worker" \
  "$REMOTE_APP_DIR/target/release/lumagg-market-data-worker-testnet"

install -m 644 "$REMOTE_SRC/deploy/lumagg-api-testnet.service" /etc/systemd/system/
install -m 644 "$REMOTE_SRC/deploy/lumagg-indexer-testnet.service" /etc/systemd/system/
install -m 644 "$REMOTE_SRC/deploy/lumagg-limit-keeper-testnet.service" /etc/systemd/system/
install -m 644 "$REMOTE_SRC/deploy/lumagg-worker-testnet.service" /etc/systemd/system/

systemctl daemon-reload
systemctl enable \
  lumagg-api-testnet \
  lumagg-indexer-testnet \
  lumagg-limit-keeper-testnet \
  lumagg-worker-testnet
systemctl restart lumagg-worker-testnet

set -a
# shellcheck disable=SC1091
source /etc/lumagg/testnet-redis.env
set +a
for _ in $(seq 1 90); do
  if [[ "$(redis-cli -u "$SNAPSHOT_REDIS_URL" --raw EXISTS lumagg:snapshot:current 2>/dev/null)" == "1" ]]; then
    break
  fi
  sleep 2
done
if [[ "$(redis-cli -u "$SNAPSHOT_REDIS_URL" --raw EXISTS lumagg:snapshot:current 2>/dev/null)" != "1" ]]; then
  echo "ERROR: testnet worker did not publish a Redis snapshot." >&2
  journalctl -u lumagg-worker-testnet -n 40 --no-pager >&2
  exit 1
fi

systemctl restart lumagg-indexer-testnet lumagg-api-testnet lumagg-limit-keeper-testnet
REMOTE

echo "=== Verify services ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "systemctl --no-pager --full status \
    lumagg-worker-testnet lumagg-api-testnet lumagg-indexer-testnet lumagg-limit-keeper-testnet | \
    sed -n '1,80p'; \
   curl -fsS http://127.0.0.1:3200/api/v1/health"

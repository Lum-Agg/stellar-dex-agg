#!/bin/bash
# Reset stuck stellar-rpc ingest DB + captive core data, then restart.
# Run on server as root after disk cleanup.
#
# Usage: ./deploy/reset_stellar_rpc_ingest.sh
#        KEEP_BACKUP=1 ./deploy/reset_stellar_rpc_ingest.sh   # move aside instead of rm
set -euo pipefail

RPC_DB="/stellar/stellar-rpc.sqlite"
CAPTIVE_DIR="/stellar/rpc-captive"
STAMP="$(date +%Y%m%d-%H%M%S)"

echo "=== Stopping services ==="
systemctl stop lumagg-worker.service 2>/dev/null || true
for port in 3100 3101 3102 3103; do
  systemctl stop "lumagg-api@${port}" 2>/dev/null || true
done
systemctl stop stellar-rpc.service
sleep 3
pkill -x stellar-core 2>/dev/null || true
sleep 2

archive_or_rm() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    return
  fi
  if [[ "${KEEP_BACKUP:-0}" == "1" ]]; then
    local dest="/stellar/archive-${STAMP}/$(basename "$path")"
    mkdir -p "$(dirname "$dest")"
    echo "Archiving $path -> $dest"
    mv "$path" "$dest"
  else
    echo "Removing $path"
    rm -rf "$path"
  fi
}

echo "=== Reset ingest state ==="
archive_or_rm "${RPC_DB}"
archive_or_rm "${RPC_DB}-shm"
archive_or_rm "${RPC_DB}-wal"
archive_or_rm "${CAPTIVE_DIR}"

mkdir -p "${CAPTIVE_DIR}"
chown -R stellar:stellar "${CAPTIVE_DIR}"

echo "=== Point lumagg at public RPC until local catches up ==="
mkdir -p /etc/systemd/system/lumagg-worker.service.d /etc/systemd/system/lumagg-api@.service.d
cat >/etc/systemd/system/lumagg-worker.service.d/rpc-override.conf <<'EOF'
[Service]
Environment=RPC_URL=https://soroban-rpc.mainnet.stellar.gateway.fm
EOF
cp /etc/systemd/system/lumagg-worker.service.d/rpc-override.conf \
  /etc/systemd/system/lumagg-api@.service.d/rpc-override.conf

systemctl daemon-reload
systemctl reset-failed stellar-rpc.service 2>/dev/null || true
systemctl start stellar-rpc.service
sleep 15

echo "=== Status ==="
df -h /stellar
systemctl is-active stellar-rpc
curl -s -m 10 -X POST http://127.0.0.1:8003/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' || true
echo
pgrep -a -x stellar-core || echo "(no core yet)"
echo ""
echo "Watch: journalctl -u stellar-rpc -f"
echo "Re-run configure_shared_core.sh to switch lumagg back to :8003 when getHealth latestLedger is near mainnet."

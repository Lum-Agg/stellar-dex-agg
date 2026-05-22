#!/bin/bash
# Config-only shared Stellar node: RPC owns the only stellar-core (:11626 / :11628).
# stellar-horizon is disabled (LumAgg Classic uses public Horizon).
#
# Usage (on server as root): ./deploy/configure_shared_core.sh
#
# Optional: SHARED_CORE_MODE=external  (experimental; often fails on stellar-rpc v26)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${SHARED_CORE_MODE:-owned}"
CORE_HTTP_PORT=11626

echo "=== Shared core setup (mode=${MODE}) ==="
echo "=== Stopping dependents ==="
systemctl stop lumagg-worker.service 2>/dev/null || true
for port in 3100 3101 3102 3103; do
  systemctl stop "lumagg-api@${port}" 2>/dev/null || true
done
systemctl stop stellar-rpc.service stellar-horizon.service 2>/dev/null || true
sleep 2

pkill -f 'stellar-core.*rpc-captive' 2>/dev/null || true
pkill -f 'stellar-core.*catchup' 2>/dev/null || true
sleep 2

mkdir -p /etc/systemd/system/stellar-rpc.service.d
cp "${SCRIPT_DIR}/stellar-rpc.service.d-config-path.conf" \
  /etc/systemd/system/stellar-rpc.service.d/config-path.conf
mkdir -p /stellar/rpc-captive
chown -R stellar:stellar /stellar/rpc-captive

if [[ "$MODE" == "external" ]]; then
  cp "${SCRIPT_DIR}/stellar-captive-core-remote.cfg" /etc/stellar/stellar-captive-core-remote.cfg
  chown stellar:stellar /etc/stellar/stellar-captive-core-remote.cfg
  cp "${SCRIPT_DIR}/soroban-rpc.external.toml" /etc/stellar/soroban-rpc.toml
  if [[ -f "${SCRIPT_DIR}/stellar-shared-core.cfg" ]]; then
    cp "${SCRIPT_DIR}/stellar-shared-core.cfg" /etc/stellar/stellar-core.cfg
    chown stellar:stellar /etc/stellar/stellar-core.cfg
  fi
  systemctl daemon-reload
  systemctl enable stellar-core.service 2>/dev/null || true
  systemctl start stellar-core.service
else
  cp "${SCRIPT_DIR}/stellar-captive-mainnet.cfg" /etc/stellar/stellar-captive-mainnet.cfg
  chown stellar:stellar /etc/stellar/stellar-captive-mainnet.cfg
  cp "${SCRIPT_DIR}/soroban-rpc.toml" /etc/stellar/soroban-rpc.toml
  chown stellar:stellar /etc/stellar/soroban-rpc.toml
  systemctl stop stellar-core.service 2>/dev/null || true
  systemctl disable stellar-core.service 2>/dev/null || true
  pkill -x stellar-core 2>/dev/null || true
  sleep 2
fi

echo "=== Disable stellar-horizon (not used; avoids extra captive core) ==="
systemctl stop stellar-horizon.service 2>/dev/null || true
systemctl disable stellar-horizon.service 2>/dev/null || true

systemctl daemon-reload

if [[ "$MODE" == "external" ]]; then
  for i in $(seq 1 24); do
    if curl -sf "http://127.0.0.1:${CORE_HTTP_PORT}/info" >/dev/null 2>&1; then
      echo "stellar-core.service /info OK"
      break
    fi
    sleep 5
    [[ "$i" -eq 24 ]] && echo "WARN: stellar-core /info not ready" >&2
  done
fi

systemctl reset-failed stellar-rpc.service 2>/dev/null || true
systemctl start stellar-rpc.service
sleep 20

RPC_HEALTH=$(curl -s --max-time 10 -X POST http://127.0.0.1:8003/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' || true)
if echo "$RPC_HEALTH" | grep -q '"status":"healthy"'; then
  echo "stellar-rpc healthy — lumagg → http://127.0.0.1:8003"
  LUM_RPC='http://127.0.0.1:8003'
else
  echo "WARN: stellar-rpc not healthy — lumagg stays on public RPC"
  LUM_RPC='https://soroban-rpc.mainnet.stellar.gateway.fm'
fi
mkdir -p /etc/systemd/system/lumagg-worker.service.d /etc/systemd/system/lumagg-api@.service.d
cat >/etc/systemd/system/lumagg-worker.service.d/rpc-override.conf <<EOF
[Service]
Environment=RPC_URL=${LUM_RPC}
EOF
cat >/etc/systemd/system/lumagg-api@.service.d/rpc-override.conf <<EOF
[Service]
Environment=RPC_URL=${LUM_RPC}
EOF
systemctl daemon-reload
systemctl start lumagg-worker.service
for port in 3100 3101 3102 3103; do systemctl start "lumagg-api@${port}" 2>/dev/null || true; done

echo ""
echo "=== Status ==="
echo "stellar-core processes: $(pgrep -c -x stellar-core 2>/dev/null || echo 0)"
pgrep -a -x stellar-core 2>/dev/null || true
systemctl is-active stellar-core.service 2>/dev/null || echo "stellar-core.service: disabled (owned)"
systemctl is-active stellar-rpc.service lumagg-worker.service || true
echo "stellar-horizon: $(systemctl is-enabled stellar-horizon.service 2>/dev/null || echo disabled)"
systemctl show stellar-rpc -p NRestarts,ActiveState --value
curl -sf "http://127.0.0.1:${CORE_HTTP_PORT}/info" 2>/dev/null | head -c 160 || echo "(core /info not ready — catchup in progress)"
echo
echo "getHealth: $RPC_HEALTH"
echo ""
echo "Architecture: stellar-rpc (:8003) → sole stellar-core (:${CORE_HTTP_PORT}); horizon disabled"

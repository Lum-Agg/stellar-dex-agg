#!/bin/bash
# Deploy Stellar DEX Aggregator to server
# Usage: ./deploy.sh

set -e

SERVER="root@178.63.81.216"
SSH_KEY="~/.ssh/id_rsa"
REMOTE_DIR="/opt/stellar-dex-aggregator"

echo "=== Building release binary ==="
cargo build -p api-server --release

echo "=== Uploading binary ==="
ssh -i $SSH_KEY $SERVER "mkdir -p $REMOTE_DIR/data"
scp -i $SSH_KEY target/release/api-server $SERVER:$REMOTE_DIR/api-server

echo "=== Uploading pool cache ==="
if [ -f data/pool_cache.json ]; then
    scp -i $SSH_KEY data/pool_cache.json $SERVER:$REMOTE_DIR/data/pool_cache.json
fi

echo "=== Creating systemd service ==="
ssh -i $SSH_KEY $SERVER "cat > /etc/systemd/system/lumagg-api.service << 'EOF'
[Unit]
Description=LumAgg DEX Aggregator API
After=network.target stellar-rpc.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/stellar-dex-aggregator
Environment=RPC_URL=http://127.0.0.1:8003
Environment=HORIZON_URL=http://127.0.0.1:8000
Environment=LISTEN_ADDR=0.0.0.0:3100
Environment=REFRESH_INTERVAL_SECS=5
Environment=RUST_LOG=info
ExecStart=/opt/stellar-dex-aggregator/api-server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF"

echo "=== Starting service ==="
ssh -i $SSH_KEY $SERVER "systemctl daemon-reload && systemctl enable lumagg-api && systemctl restart lumagg-api && sleep 2 && systemctl status lumagg-api | head -8"

echo "=== Testing ==="
sleep 3
curl -s http://178.63.81.216:3100/api/v1/health && echo " ✓"

echo "=== Done ==="

#!/usr/bin/env bash
# Smoke test LumAgg release tarballs after packaging.
#
# Usage:
#   ./scripts/smoke-release-archives.sh
#   DIST_DIR=./dist ./scripts/smoke-release-archives.sh
#
# The check is intentionally offline. It always verifies archive contents. On
# Linux x86_64 it also verifies binary startup and TOML parsing without requiring
# RPC, Redis, or chain access.
set -euo pipefail

DIST_DIR="${DIST_DIR:-.}"
SWAP_ARCHIVE="${SWAP_ARCHIVE:-lumagg-swap-api-linux-x86_64.tar.gz}"
AGG_ARCHIVE="${AGG_ARCHIVE:-lumagg-aggregator-linux-x86_64.tar.gz}"
ARB_ARCHIVE="${ARB_ARCHIVE:-lumagg-arbitrage-bot-linux-x86_64.tar.gz}"

smoke_dir="$(mktemp -d)"
trap 'rm -rf "$smoke_dir"' EXIT

tar -xzf "$DIST_DIR/$SWAP_ARCHIVE" -C "$smoke_dir"
tar -xzf "$DIST_DIR/$AGG_ARCHIVE" -C "$smoke_dir"
tar -xzf "$DIST_DIR/$ARB_ARCHIVE" -C "$smoke_dir"

swap="$smoke_dir/${SWAP_ARCHIVE%.tar.gz}"
cluster="$smoke_dir/${AGG_ARCHIVE%.tar.gz}"
arb="$smoke_dir/${ARB_ARCHIVE%.tar.gz}"

test -x "$swap/lumagg-swap-api"
test -f "$swap/README.md"
test -f "$swap/openapi.yaml"
test -f "$swap/lumagg-swap-api.toml"
test -f "$swap/lumagg-swap-api.service"

test -x "$cluster/lumagg-api-server"
test -x "$cluster/lumagg-market-data-worker"
test -x "$cluster/lumagg-analytics-indexer"
test -f "$cluster/README.md"
test -f "$cluster/lumagg-aggregator.toml"
test -f "$cluster/aggregator-configuration.md"
test -f "$cluster/analytics-indexer.md"
test -f "$cluster/openapi.yaml"
test -f "$cluster/systemd/lumagg-api@.service"
test -f "$cluster/systemd/lumagg-market-data-worker.service"
test -f "$cluster/systemd/lumagg-analytics-indexer.service"

test -x "$arb/lumagg-arbitrage-bot"
test -f "$arb/README.md"
test -f "$arb/arbitrage-configuration.md"
test -f "$arb/lumagg-arbitrage.toml"
test -f "$arb/lumagg-arbitrage.service"

host_os="$(uname -s)"
host_arch="$(uname -m)"
if [[ "${EXECUTE_BINARIES:-auto}" == "0" || "${EXECUTE_BINARIES:-auto}" == "false" ]]; then
  echo "archive structure smoke test passed; binary execution skipped"
  exit 0
fi
if [[ "${EXECUTE_BINARIES:-auto}" == "auto" && ! ( "$host_os" == "Linux" && "$host_arch" == "x86_64" ) ]]; then
  echo "archive structure smoke test passed; binary execution skipped on $host_os/$host_arch"
  exit 0
fi

"$swap/lumagg-swap-api" --version
"$cluster/lumagg-api-server" --version
"$cluster/lumagg-market-data-worker" --version
"$cluster/lumagg-analytics-indexer" --version
"$arb/lumagg-arbitrage-bot" --version

cp "$swap/lumagg-swap-api.toml" "$smoke_dir/swap-api.toml"
"$swap/lumagg-swap-api" --config "$smoke_dir/swap-api.toml" --check-config

cp "$cluster/lumagg-aggregator.toml" "$smoke_dir/aggregator.toml"
perl -pi -e 's/aggregator_contract = "CHANGE_ME"/aggregator_contract = "CDUMMYAGGREGATORCONTRACT000000000000000000000000000000000000"/' "$smoke_dir/aggregator.toml"
"$cluster/lumagg-market-data-worker" --config "$smoke_dir/aggregator.toml" --check-config
"$cluster/lumagg-api-server" --config "$smoke_dir/aggregator.toml" --check-config
"$cluster/lumagg-analytics-indexer" --config "$smoke_dir/aggregator.toml" --check-config

cp "$arb/lumagg-arbitrage.toml" "$smoke_dir/arbitrage.toml"
perl -pi -e 's/aggregator = "CHANGE_ME"/aggregator = "CDUMMYAGGREGATORCONTRACT000000000000000000000000000000000000"/' "$smoke_dir/arbitrage.toml"
perl -pi -e 's/bridge_tokens = \["CHANGE_ME"\]/bridge_tokens = ["CDUMMYBRIDGETOKEN000000000000000000000000000000000000000"]/' "$smoke_dir/arbitrage.toml"
"$arb/lumagg-arbitrage-bot" --config "$smoke_dir/arbitrage.toml" --check-config

echo "release archive smoke test passed"

# Deployment Overview

LumAgg releases native Linux binaries from the monorepo. Docker can still be
used for local testing, but the production path is to run the binaries directly
under your preferred process manager.

## Which binary should I run?

| Goal | Binary or binaries | Notes |
| --- | --- | --- |
| Private quote API or integration testing | `lumagg-swap-api` | One process runs the API and market-data worker with in-memory state. Redis is not required. |
| Public or horizontally scaled aggregator | `lumagg-market-data-worker` + `lumagg-api-server` | The worker publishes market state to Redis; one or more API processes read from Redis. |
| Public analytics, swap history, and arbitrage history | `lumagg-analytics-indexer` | Reads chain events into SQLite; API serves `/stats`, `/swaps`, and `/arbitrage` from that DB. |
| Atomic round-trip arbitrage | `lumagg-arbitrage-bot` | Runs separately from the quote stack and should use low-latency quote API and RPC endpoints. |

## Recommended topologies

### Self-hosted quote API

```text
lumagg-swap-api
  -> Soroban RPC
  -> Aggregator contract for /build_tx
```

Use this when you want the smallest deployable quote service. It is the easiest
way for wallets, bots, and integrators to test LumAgg routing without operating
Redis.

Guide: [LumAgg Swap API](lumagg-swap-api.md)

### Production aggregator

```text
lumagg-market-data-worker -> Redis -> lumagg-api-server x N
lumagg-analytics-indexer -> SQLite -> lumagg-api-server x N
```

Use this when API replicas need shared market state, when a public endpoint must
scale, or when an arbitrage operator needs a stable local quote plane.

Guide: [Production Aggregator Deployment](aggregator-deployment.md)

### Arbitrage operator

```text
LumAgg quote API -> lumagg-arbitrage-bot -> Soroban RPC -> Vault / Aggregator
```

Run the arbitrage bot as its own process. It scans, simulates, and optionally
submits atomic round-trip transactions. Keep it close to both the quote API and
RPC; latency directly affects opportunity validity.

Guide: [Arbitrage Deployment](arbitrage-deployment.md)

Configuration: [Arbitrage Configuration](arbitrage-configuration.md)

## Configuration files

LumAgg uses TOML files rather than requiring environment variables:

| Config | Used by |
| --- | --- |
| `lumagg-swap-api.toml` | `lumagg-swap-api` |
| `lumagg-aggregator.toml` | `lumagg-market-data-worker`, `lumagg-api-server`, `lumagg-analytics-indexer` |
| `lumagg-arbitrage.toml` | `lumagg-arbitrage-bot` |

Release archives include complete templates. Store edited configs outside the
repository, restrict file permissions, and keep private RPC URLs, Redis
passwords, partner API keys, Telegram credentials, and caller secrets out of
Git.

Reference: [Aggregator Configuration](aggregator-configuration.md)

## Release archive smoke test

After downloading all three release archives and `SHA256SUMS`, you can verify
the package structure and config templates without connecting to RPC or Redis:

```bash
git clone https://github.com/Lum-Agg/stellar-dex-agg.git
cd stellar-dex-agg
./scripts/smoke-release-archives.sh
```

Run it from the directory containing:

- `lumagg-swap-api-linux-x86_64.tar.gz`
- `lumagg-aggregator-linux-x86_64.tar.gz`
- `lumagg-arbitrage-bot-linux-x86_64.tar.gz`

Use `DIST_DIR=/path/to/downloads ./scripts/smoke-release-archives.sh` when the
archives are in another directory. The script always checks required files. On
Linux x86_64 it also checks binary `--version` and `--check-config` against
patched dummy contract IDs. On macOS or other non-Linux hosts it skips binary
execution because the release archives contain Linux x86_64 binaries.

## Validation checklist

After deployment:

```bash
./lumagg-market-data-worker --config ./aggregator.toml --check-config
./lumagg-api-server --config ./aggregator.toml --check-config
./lumagg-analytics-indexer --config ./aggregator.toml --check-config
./lumagg-arbitrage-bot --config ./arbitrage.toml --check-config
```

Then verify the public API surface:

```bash
curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/ready
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
curl -fsS http://127.0.0.1:3100/api/v1/stats | jq
```

`/health` means the process is alive. `/ready` means route data has loaded and
the API can quote.

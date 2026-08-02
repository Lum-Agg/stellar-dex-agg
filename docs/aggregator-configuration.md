# Aggregator Configuration Reference

`lumagg-api-server` and `lumagg-market-data-worker` are configured through environment
variables. The release archive includes `aggregator.env.example`, which is a
shell-compatible, systemd-compatible starting point. Copy it to a private file,
edit the values, and load it before starting either process:

```bash
cp aggregator.env.example aggregator.env
chmod 600 aggregator.env
set -a
. ./aggregator.env
set +a
```

The two processes may use separate files, but all `SNAPSHOT_*`, `RPC_URL`, and
`NETWORK_PASSPHRASE` values must agree. Do not commit Redis credentials,
partner keys, or Telegram credentials.

## Required production settings

| Variable | Example | Used by | Description |
| --- | --- | --- | --- |
| `RPC_URL` | `https://rpc.example.com` | Both | Soroban JSON-RPC endpoint. Use a capacity-controlled endpoint for production. |
| `NETWORK_PASSPHRASE` | `Public Global Stellar Network ; September 2015` | Both | Stellar network identity. Must match the RPC and deployed contracts. |
| `SNAPSHOT_BACKEND` | `redis` | Both | Shared production backend. Supported values are `redis`, `file`, and `memory`; use `redis` for the split topology. |
| `SNAPSHOT_REDIS_URL` | `redis://:password@127.0.0.1:6379/` | Both | Private Redis connection URL. Percent-encode reserved password characters. |
| `AGGREGATOR_CONTRACT` | `C...` | API | Aggregator contract used by `/build_tx`. Omit only for a quote-only API. |
| `LISTEN_ADDR` | `127.0.0.1:3100` | API | HTTP bind address. Put public TLS traffic behind a reverse proxy or load balancer. |

## Shared storage

| Variable | Default | Description |
| --- | --- | --- |
| `LUMAGG_MODE` | `cluster` | `cluster` loads worker snapshots through Redis. `lumagg-swap-api` forces `embedded`; do not use embedded mode for the separate binaries. |
| `SNAPSHOT_REDIS_CHANNEL` | `lumagg:snapshot:events` | Redis Pub/Sub notification channel. It must match on every process. |
| `SNAPSHOT_REDIS_KEEP_LATEST` | `10` | Number of topology snapshot versions retained in Redis. |
| `SNAPSHOT_POLL_INTERVAL_MS` | `1000` | API fallback interval for checking for a newer topology snapshot. Minimum is 1 ms. |
| `POOL_STATE_TTL_SECS` | `86400` | TTL of live pool-state entries written to Redis. |
| `SNAPSHOT_DIR` | `data/snapshots` | File-backend directory. Not used when the backend is Redis. |

## Market-data worker

| Variable | Default | Description |
| --- | --- | --- |
| `ENABLED_DEX_SOURCES` | all | Optional comma-separated adapter allowlist: `aquarius`, `aquarius_clmm`, `soroswap`, `phoenix`, `sushi`, `comet`, `classic_dex`. |
| `DISCOVERY_INTERVAL_SECS` | `600` | Full pool and topology rediscovery interval. |
| `REFRESH_INTERVAL_SECS` | `30` | Full reserve refresh interval for the standalone worker. |
| `POOL_PUBLISH_INTERVAL_SECS` | `2` | Cache-to-Redis publish interval used by the legacy pipeline. |
| `POOL_STATE_REFRESH_CONCURRENCY` | `8` | Concurrent RPC batches during xy=k reserve refresh. |
| `LEDGER_WATCHER_ENABLED` | `true` | Enables event-driven detection of pools touched by new ledgers. |
| `LEDGER_POLL_SECS` | `0.1` | Ledger polling interval. Fractional seconds are accepted; values below 0.1 are clamped. |
| `LEDGER_MAX_CATCHUP` | `32` | Maximum recent ledgers processed after the watcher falls behind. |
| `LEDGER_MAX_TOUCHED_REFRESH` | `64` | Maximum touched pools refreshed in one watcher cycle. |
| `FETCH_PIPELINE_ENABLED` | `true` | Enables the event-driven RPC-to-Redis fetch pipeline. |
| `FETCH_WORKER_COUNT` | `8` | Number of fetch workers; minimum 1. Tune against RPC capacity. |
| `FETCH_HIGH_QUEUE_CAPACITY` | `512` | Touched-pool queue capacity; minimum 64. |
| `FETCH_STATS_INTERVAL_SECS` | `60` | Fetch-pipeline metrics log interval; minimum 15 seconds. |
| `AQUARIUS_HYDRATE_CONCURRENCY` | `16` | Aquarius CLMM hydration concurrency. |
| `HORIZON_URL` | `https://horizon.stellar.org` | Horizon endpoint used by the Classic DEX adapter. |
| `SOROSWAP_FACTORY_CONTRACT` | built-in mainnet address | Optional Soroswap factory override. |
| `COMET_FACTORY` | built-in mainnet address | Optional Comet factory override. |
| `COMET_EXTRA_POOLS` | empty | Optional comma-separated Comet pool addresses. |
| `COMET_FACTORY_EVENTS_LEDGER_WINDOW` | `50000` | Recent ledger window scanned for Comet factory pool events. |
| `SUSHI_DISCOVERY_RPC` | public mainnet RPC | Optional dedicated RPC for Sushi pool discovery. |

## API and routing

| Variable | Default | Description |
| --- | --- | --- |
| `SPLIT_THRESHOLD_BPS` | `5` | Price-impact threshold for attempting split optimization. |
| `SPLIT_COMPETITIVE_DELTA_BPS` | `50` | Also try splitting when the second path is within this many bps of the best path. |
| `MIN_SPLIT_FRACTION_BPS` | `5` | Removes split legs below this share of total expected output. |
| `MAX_SPLITS` | `3` | Maximum candidate paths considered by split optimization. Requests may lower it. |
| `PATH_FINDER_MAX_HOPS` | `3` | Maximum hops per route. Requests may lower it. |
| `PATH_FINDER_MAX_MULTI_HOP_PATHS` | `50` | Candidate cap for routes with two or more hops. |
| `PATH_FINDER_MAX_DIRECT_PATHS` | `0` | Candidate cap for direct pools; `0` means all direct pools. |
| `QUOTE_RPC_HYDRATE_ENABLED` | `false` | Allows API-side RPC hydration on Redis pool-state misses. Keep false when the worker is healthy. |
| `QUOTE_HYDRATE_MAX_POOLS` | `12` | Maximum xy=k pools hydrated through RPC per quote. Minimum 1. |
| `QUOTE_ON_CHAIN_VALIDATE` | `false` | Runs optional on-chain hop validation by default. A request can override this. |
| `INSTRUCTION_LEEWAY` | `100000000` | Extra CPU instruction budget requested while simulating and assembling transactions. |
| `TOKEN_LOGO_DIR` | `data/logos` | Local directory served by the token-logo endpoint. |
| `TOKEN_LOGO_BASE_URL` | `https://api.lumagg.xyz/logos` | Public base URL written into locally resolved token metadata. |
| `TOKEN_LOGO_LIST_URLS` | built-in lists | Optional comma-separated external token-list URLs. |
| `RUST_LOG` | binary-specific `info` filters | Standard tracing filter, for example `api_server=info,router_engine=info`. |

The API has a fixed public limit of 10 requests/second per IP and 60
requests/second per partner key:

| Variable | Default | Description |
| --- | --- | --- |
| `LUMAGG_PARTNER_API_KEYS` | empty | Comma-separated accepted `X-API-Key` values. When configured, unknown keys are rejected. |
| `QUOTE_RATE_LIMIT_BYPASS_IPS` | loopback only | Comma-separated additional IP addresses that bypass the public-IP bucket. CIDR notation is not supported. |

## Optional endpoints and price data

| Variable | Default | Description |
| --- | --- | --- |
| `ESCROW_CONTRACT` | unset | Order Escrow contract used by limit/DCA transaction builders. |
| `INDEXER_DB_PATH` | unset | Analytics indexer SQLite path for stats, swaps, arbitrage, and order-history endpoints. `LUMAGG_INDEXER_DB_PATH` is a legacy alias. |
| `PRICE_DB_PATH` | unset | SQLite price-mark store. Sampling starts only when this is set. |
| `PRICE_SAMPLER` | enabled | Set to `0` to disable sampling while retaining read access to `PRICE_DB_PATH`. |
| `PRICE_SAMPLE_SECS` | `600` | Price sampling interval. |
| `PRICE_SAMPLE_TOKEN_LIMIT` | `30` | Number of common tokens sampled in addition to priority tokens. |
| `PRICE_RETENTION_DAYS` | unlimited | Optional retention window for sampled price ticks. |

## Optional monitoring

| Variable | Default | Description |
| --- | --- | --- |
| `TELEGRAM_ALERTS_ENABLED` | `false` | Enables Telegram alerts. Requires both credentials below. |
| `TELEGRAM_BOT_TOKEN` | unset | Telegram bot token. Treat as a secret. |
| `TELEGRAM_CHAT_ID` | unset | Destination Telegram chat ID. |
| `TELEGRAM_PRIMARY_API_PORT` | `3100` | Only this API replica sends API-side alerts. |
| `TELEGRAM_HEARTBEAT_INTERVAL_SECS` | `600` | Worker heartbeat interval; minimum 60 seconds. |
| `MONITOR_API_HEALTH_URL` | `http://127.0.0.1:3100/api/v1/health` | API health URL included in worker monitoring. |
| `MAINNET_RPC_REF_URL` | public mainnet RPC | Reference RPC used to detect ledger lag. |
| `QUOTE_REDIS_MISS_ALERT_MIN` | `12` | Minimum Soroswap pool-state misses before an alert is considered. |
| `QUOTE_REDIS_MISS_ALERT_RATIO_BPS` | `3000` | Minimum missed share, in bps, before an alert is sent. |

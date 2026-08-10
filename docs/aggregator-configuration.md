# Aggregator Configuration Reference

`lumagg-api-server`, `lumagg-market-data-worker`, and
`lumagg-analytics-indexer` read the same TOML file.
The release archive includes a complete `lumagg-aggregator.toml` template.
Unknown sections and unknown keys are rejected to catch spelling mistakes.

```bash
./lumagg-market-data-worker --config ./aggregator.toml --check-config
./lumagg-api-server --config ./aggregator.toml --check-config
./lumagg-analytics-indexer --config ./aggregator.toml --check-config
```

The file can contain Redis credentials, partner keys, and Telegram credentials.
Store it outside the repository and restrict it to the service account:

```bash
chmod 600 aggregator.toml
```

## Process usage

All three binaries accept the same file, but each process uses only the
sections relevant to its role:

| Process | Main sections used | Does not require |
| --- | --- | --- |
| `lumagg-market-data-worker` | `network`, `redis`, `worker`, `dex`, `monitoring` | `api.aggregator_contract`, `indexer` |
| `lumagg-api-server` | `network`, `redis`, `api`, `routing`, `dex`, `access`, `features`, `indexer`, `monitoring` | `worker` runtime settings |
| `lumagg-analytics-indexer` | `network`, `dex`, `indexer`, `monitoring` | `redis`, `worker`, `api` |

Keeping one shared file prevents network, contract, and database settings from
drifting between processes. It does not mean every section is required by
every binary.

## Network and Redis

| TOML key | Required/default | Description |
| --- | --- | --- |
| `network.rpc_url` | required | Soroban JSON-RPC endpoint. Use a capacity-controlled endpoint in production. This can be hosted separately from Horizon. |
| `network.passphrase` | mainnet | Stellar network passphrase. It must match the RPC and deployed contracts. |
| `redis.url` | required | Private Redis URL. Percent-encode reserved password characters. |
| `redis.channel` | `lumagg:snapshot:events` | Snapshot Pub/Sub notification channel. |
| `redis.keep_latest` | `10` | Number of topology snapshot versions retained. Must be greater than zero. |
| `redis.poll_interval_ms` | `1000` | API fallback interval for checking for a new topology snapshot. |
| `redis.pool_state_ttl_secs` | `86400` | TTL of live pool-state entries. |

## Worker

| TOML key | Program default | Description |
| --- | --- | --- |
| `worker.enabled_dex_sources` | all | Adapter allowlist: `aquarius`, `aquarius_clmm`, `soroswap`, `phoenix`, `sushi`, `comet`, `classic_dex`. |
| `worker.discovery_interval_secs` | `600` | Full pool and topology rediscovery interval. |
| `worker.refresh_interval_secs` | `30` | Full reserve refresh interval. |
| `worker.pool_publish_interval_secs` | `2` | Cache-to-Redis interval used by the legacy pipeline. |
| `worker.pool_state_refresh_concurrency` | `8` | Concurrent RPC batches during reserve refresh. |
| `worker.ledger_watcher_enabled` | `true` | Enables event-driven touched-pool detection. |
| `worker.ledger_poll_secs` | `0.1` | Ledger polling interval; values below 0.1 are clamped. |
| `worker.ledger_max_catchup` | `32` | Maximum recent ledgers processed after lag. |
| `worker.ledger_max_touched_refresh` | `64` | Maximum touched pools refreshed per cycle. |
| `worker.fetch_pipeline_enabled` | `true` | Enables the event-driven RPC-to-Redis pipeline. |
| `worker.fetch_worker_count` | `8` | Fetch worker count. Tune against RPC capacity. |
| `worker.fetch_high_queue_capacity` | `512` | Touched-pool queue capacity. |
| `worker.fetch_stats_interval_secs` | `60` | Pipeline metrics log interval. |

## API and routing

| TOML key | Program default | Description |
| --- | --- | --- |
| `api.listen_addr` | `0.0.0.0:3100` | HTTP bind address. `--listen-addr` overrides it for a replica. |
| `api.aggregator_contract` | unset | Contract used by `/build_tx`; omit for quote-only operation. |
| `api.token_logo_dir` | `data/logos` | Local directory served under `/logos`. |
| `api.token_logo_base_url` | LumAgg API | Public URL written into resolved token metadata. |
| `api.token_logo_list_urls` | built-in lists | Optional external token-list URLs. |
| `api.instruction_leeway` | `100000000` | Extra simulation CPU instruction budget. |
| `api.quote_on_chain_validate` | `false` | Enables on-chain hop validation by default. |
| `routing.split_threshold_bps` | `5` | Price-impact threshold for attempting a split. |
| `routing.split_competitive_delta_bps` | `50` | Split when the second path is within this delta. |
| `routing.min_split_fraction_bps` | `5` | Removes very small split legs. |
| `routing.max_splits` | `3` | Maximum candidate paths used by split optimization. |
| `routing.max_hops` | `3` | Maximum hops per route. |
| `routing.max_multi_hop_paths` | `50` | Candidate cap for multi-hop routes. |
| `routing.max_direct_paths` | `0` | Direct-pool cap; zero means all. |
| `routing.quote_rpc_hydrate_enabled` | `false` | Allows API-side RPC hydration after Redis misses. |
| `routing.quote_hydrate_max_pools` | `12` | Maximum pools hydrated through RPC per quote. |

## DEX adapters

| TOML key | Program default | Description |
| --- | --- | --- |
| `dex.aquarius_hydrate_concurrency` | `16` | Aquarius CLMM hydration concurrency. |
| `dex.horizon_url` | public Horizon | Horizon endpoint for the Classic DEX adapter and analytics envelope repair fallback. It is independent of `network.rpc_url`; use your own Horizon host when available. |
| `dex.soroswap_factory_contract` | built-in mainnet address | Soroswap factory override. |
| `dex.comet_factory` | built-in mainnet address | Comet factory override. |
| `dex.comet_extra_pools` | empty | Additional Comet pool addresses. |
| `dex.comet_factory_events_ledger_window` | `50000` | Recent factory-event scan window. |
| `dex.sushi_discovery_rpc` | public mainnet RPC | Dedicated Sushi discovery RPC. |

## Access and optional features

| TOML key | Program default | Description |
| --- | --- | --- |
| `access.partner_api_keys` | empty | Accepted `X-API-Key` values. Treat them as secrets. |
| `access.rate_limit_bypass_ips` | loopback only | Additional exact IPs bypassing the public-IP bucket. |
| `features.escrow_contract` | unset | Order Escrow contract for limit and DCA builders. |
| `features.price_db_path` | unset | SQLite price-mark store. |
| `features.price_sampler_enabled` | `true` | Enables sampling when a price DB is configured. |
| `features.price_sample_secs` | `600` | Price sampling interval. |
| `features.price_sample_token_limit` | `30` | Common tokens sampled beyond priority tokens. |
| `features.price_retention_days` | unlimited | Optional sampled-tick retention. |

## Analytics indexer

The entire `[indexer]` section is optional. When present, the API reads the
same SQLite database used by `lumagg-analytics-indexer` for stats, swap history,
and order history.

| TOML key | Program default | Description |
| --- | --- | --- |
| `indexer.db_path` | `./data/analytics-indexer.db` | Shared analytics SQLite path. |
| `indexer.mode` | `events` | Ingestion mode: `events`, `envelope`, or `both`. |
| `indexer.envelope_fallback` | `false` | Also inspect transaction envelopes for legacy events. |
| `indexer.poll_secs` | `30` | Delay between live ingestion polls. |
| `indexer.page_limit` | `10000` | Maximum events requested per RPC page. |
| `indexer.start_ledger` | unset | Initial ledger when the database has no cursor. |

When `indexer.envelope_fallback = true`, the indexer first tries Soroban RPC
transaction data and then falls back to `dex.horizon_url` when old transaction
envelopes are no longer available from the RPC. Use an archive-capable Horizon
for historical repairs.

## Monitoring

| TOML key | Program default | Description |
| --- | --- | --- |
| `monitoring.log_filter` | `info` | Rust tracing filter. |
| `monitoring.telegram_enabled` | `false` | Enables Telegram alerts. |
| `monitoring.telegram_bot_token` | unset | Telegram bot token. Treat it as a secret. |
| `monitoring.telegram_chat_id` | unset | Destination chat ID. |
| `monitoring.telegram_primary_api_port` | `3100` | Only this API replica sends API-side alerts. |
| `monitoring.telegram_heartbeat_interval_secs` | `600` | Worker heartbeat interval. |
| `monitoring.api_health_url` | local port 3100 | API health URL checked by worker monitoring. |
| `monitoring.mainnet_rpc_ref_url` | public mainnet RPC | Reference RPC for ledger-lag checks. |
| `monitoring.quote_redis_miss_alert_min` | `12` | Minimum Redis misses before considering an alert. |
| `monitoring.quote_redis_miss_alert_ratio_bps` | `3000` | Minimum missed share before alerting. |

# Pool state architecture

LumAgg separates **routing graph** from **per-pool live state**. Pool freshness follows an **event-driven** model (similar to Jupiter on Solana): update Redis when chain state changes or on discovery/bootstrap — not via periodic full-market sweeps.

## Data layers

| Layer | What | Storage | Update cadence |
|-------|------|---------|----------------|
| Graph | Token pairs, pool addresses, fees; CLMM `clmm_pool_refs` (topology only) | `MarketSnapshot` (`lumagg:snapshot:*`) | Discovery ~600s; API hot-reload via Pub/Sub |
| xy=k reserves | `reserve_a` / `reserve_b` per pool | `lumagg:pool:xyk:{source}:{pool_address}` | Bootstrap + discovery publish; **ledger touched** |
| Aquarius N-token | tokens, reserves, stable params | `lumagg:pool:aquarius:{pool_address}` | Same as xy=k |
| CLMM | slot0, liquidity, ticks, coverage | `lumagg:pool:clmm:{source}:{pool_address}` | Same; CLMM only if `coverage.is_complete` |

Redis pool keys use **`EX=86400`** (24h) by default. TTL is cache eviction, not a freshness scheduler — untouched pools keep the last known-good state until the next write.

There is **no long-lived in-process pool cache** on the API. Each `/quote` does one Redis `MGET` for pools on candidate paths. Optional API RPC hydrate only when `QUOTE_RPC_HYDRATE_ENABLED=true` (emergency).

## Update paths (three channels)

```text
┌─────────────────────────────────────────────────────────────────┐
│ 1. Bootstrap (worker start)                                      │
│    Seed topology from Redis snapshot → background discovery      │
│    → publish MarketSnapshot + full pool state to Redis           │
├─────────────────────────────────────────────────────────────────┤
│ 2. Hot — ledger watcher (LEDGER_POLL_SECS, default 0.1)         │
│    getEvents → touched known pools → fetch pipeline → Redis      │
│    Active pools: ~0.5–2s latency                                 │
├─────────────────────────────────────────────────────────────────┤
│ 3. Cold — discovery (DISCOVERY_INTERVAL_SECS, default 600)       │
│    Full adapter discovery → snapshot + full pool state publish   │
│    Reconciliation, new pools, missed events                    │
└─────────────────────────────────────────────────────────────────┘
```

No periodic full-market pool refresh loop. Pools with **no on-chain activity** are not re-fetched; their Redis values remain correct until overwritten.

## Quote flow (`/api/v1/quote`)

```text
1. find_paths          — graph only (all candidate paths; no liquidity prune)
2. collect pool keys   — unique (source, pool_address) across candidate paths
3. Redis MGET          — one round trip for xy=k + Aquarius + CLMM keys
4. quote paths         — local math only (QUOTE_RPC_HYDRATE_ENABLED=false by default)
```

## Fetch pipeline (ledger hot path)

When `FETCH_PIPELINE_ENABLED=true` (default):

```text
Ledger watcher enqueue_touched
  → high-priority FetchTask queue
  → RPC workers (FETCH_WORKER_COUNT, default 8)
  → Redis sink (set_xyk_batch / set_aquarius_batch / set_clmm_batch)
```

Legacy `POOL_PUBLISH_INTERVAL_SECS` loop is disabled while the fetch pipeline is on.

## Ledger watcher

There is no Soroban WebSocket / Geyser equivalent; the worker **polls** RPC:

1. `getLatestLedger` — detect new `sequence`
2. `getEvents` on `[last+1, latest]` with contract filter
3. `contractId` intersected with known pool index (graph + CLMM)
4. Touched pools → fetch pipeline (or legacy `touched_refresh` if pipeline off)
5. Redis writeback — xy=k / Aquarius always; CLMM only if `is_complete`

Ledger ticks do **not** publish a new `MarketSnapshot`.

| Variable | Default | Meaning |
|----------|---------|---------|
| `LEDGER_WATCHER_ENABLED` | `true` (requires Redis) | Turn ledger poll on/off |
| `LEDGER_POLL_SECS` | `0.1` | Poll interval (fractional seconds; min `0.1`) |
| `LEDGER_MAX_CATCHUP` | `32` | Max ledgers per poll |
| `LEDGER_MAX_TOUCHED_REFRESH` | `64` | Cap pools refreshed per poll |
| `FETCH_PIPELINE_ENABLED` | `true` | Ledger → task queue → Redis |
| `FETCH_WORKER_COUNT` | `8` | Concurrent RPC fetch workers |
| `FETCH_STATS_INTERVAL_SECS` | `60` | Pipeline metrics log interval |

Code: `crates/market-data-worker/src/fetch_pipeline.rs`, `ledger_watcher.rs`, `touched_refresh.rs`.

## CLMM write-back policy

Incomplete tick windows must not be shared across API instances.

- **`coverage.is_complete == true`** → worker may `SET lumagg:pool:clmm:...`
- **`is_complete == false`** → do **not** write Redis; quote engine skips those hops

CLMM tick data lives in **pool contract storage** (not separate accounts). Aquarius: `TickChunk` / bitmap keys; Sushi: pool storage read via pool-lens.

Implemented in `market_snapshot::pool_state_store::should_publish_clmm_to_redis`.

## Configuration (environment)

| Variable | Default | Meaning |
|----------|---------|---------|
| `POOL_STATE_TTL_SECS` | `86400` | Redis EX on pool keys (eviction, not freshness SLA) |
| `DISCOVERY_INTERVAL_SECS` | `600` | Full discovery + reconciliation publish |
| `POOL_STATE_REFRESH_CONCURRENCY` | `8` | Concurrent getLedgerEntries batches (ledger path) |
| `QUOTE_RPC_HYDRATE_ENABLED` | `false` | API: RPC on Redis miss (emergency) |
| `QUOTE_HYDRATE_MAX_POOLS` | `12` | API: max xy=k RPC hydrates when enabled |
| `SNAPSHOT_REDIS_URL` | — | Required for pool state store |

## Related code

- `crates/market-snapshot/src/pool_state_store.rs` — keys, TTL, publish/MGET, CLMM policy
- `crates/market-data-worker/src/worker.rs` — bootstrap, discovery, ledger integration
- `crates/market-data-worker/src/fetch_pipeline.rs` — ledger-driven fetch → Redis
- `crates/api-server/src/pool_hydrate.rs` — quote hydration from Redis
- `crates/router-engine/src/quote_engine.rs` — local quote math

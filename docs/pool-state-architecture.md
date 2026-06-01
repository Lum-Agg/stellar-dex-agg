# Pool state architecture

LumAgg separates **routing graph** from **per-pool live state**. The **market-data worker** refreshes pool state on-chain every ~2s (parallel RPC) and writes Redis; the **API** reads Redis only (`QUOTE_RPC_HYDRATE_ENABLED=false` by default).

## Data layers

| Layer | What | Storage | TTL / update |
|-------|------|---------|----------------|
| Graph | Token pairs, pool addresses, fees; CLMM `clmm_pool_refs` (topology only — no reserves/ticks) | `MarketSnapshot` (`lumagg:snapshot:*` or file) | Discovery ~600s; versioned snapshot reload on API |
| xy=k reserves | `reserve_a` / `reserve_b` per pool | `lumagg:pool:xyk:{source}:{pool_address}` | **EX=30** (default); worker refresh + quote hydrate writeback |
| CLMM | slot0, liquidity, ticks, coverage | `lumagg:pool:clmm:{source}:{pool_address}` | **EX=30** (default); worker refresh; quote read + selective writeback |

There is **no long-lived in-process pool state cache** on the API. Each `/quote` does one Redis `MGET` for all pools on candidate paths (no path prune). Optional API RPC hydrate only when `QUOTE_RPC_HYDRATE_ENABLED=true`.

## Quote flow (`/api/v1/quote`)

```text
1. find_paths          — graph only (all candidate paths; no liquidity prune)
2. collect pool keys   — unique (source, pool_address) across candidate paths
3. Redis MGET          — one round trip for xy=k + CLMM keys
4. quote paths         — local math only (paths with Redis misses simulate without reserves)
```

Worker (every ~2s, parallel):

```text
1. refresh_reserves    — all adapters concurrently; Soroswap/Aquarius use parallel getLedgerEntries batches
2. CLMM slot0/ticks    — Sushi + Aquarius CLMM in parallel
3. Redis SET           — lumagg:pool:xyk:* and complete lumagg:pool:clmm:*
```

## CLMM write-back policy

Incomplete tick windows must not be shared across API instances.

- **`coverage.is_complete == true`** → worker and quote hydrate may `SET` `lumagg:pool:clmm:...` with EX=8.
- **`is_complete == false` or missing coverage** → do **not** write Redis; quote may still use in-request hydration if the engine already has coverage guards (`clmm_swap_allowed`).
- Quote-time CLMM RPC refresh is intentionally limited; the worker remains the primary source of complete CLMM snapshots.

Implemented in `market_snapshot::pool_state_store::should_publish_clmm_to_redis`.

## Worker (hot path)

On each refresh/discovery cycle:

1. Publish topology-only `MarketSnapshot` (pairs + `clmm_pool_refs`, no reserves).
2. From adapter caches, publish xy=k reserves to `lumagg:pool:xyk:*` (EX=8).
3. Publish complete CLMM pools that pass `should_publish_clmm_to_redis` to `lumagg:pool:clmm:*` (EX=8).

## Ledger watcher (worker)

There is no chain-native WebSocket subscription; the worker **polls** Soroban RPC:

1. `getLatestLedger` — detect new `sequence`
2. `getEvents` on `[last+1, latest]` with a contract filter (`topics: [["**"]]`, all emitters)
3. `contractId` intersected with the known pool index (graph + CLMM addresses)
4. **Partial refresh** — batch `getLedgerEntries` for touched xy=k pools; `ensure_pool_loaded` for touched CLMM
5. **Redis writeback** — `set_xyk_batch` / `set_clmm_batch` (CLMM only if `is_complete`)

Full-market `refresh_interval` + snapshot publish remain as a safety net. Ledger ticks update Redis only (no extra snapshot publish).

| Variable | Default | Meaning |
|----------|---------|---------|
| `LEDGER_WATCHER_ENABLED` | `true` (requires Redis) | Turn ledger poll on/off |
| `LEDGER_POLL_SECS` | `0.5` | Poll interval (fractional seconds; min `0.1`) |
| `LEDGER_MAX_CATCHUP` | `32` | Max ledgers per poll |
| `LEDGER_MAX_TOUCHED_REFRESH` | `64` | Cap pools refreshed per poll |

Code: `crates/market-data-worker/src/ledger_watcher.rs`, `touched_refresh.rs`, `crates/dex-adapters/src/rpc/events.rs`, `pool_index.rs`.

### Phoenix / Comet refresh

| Source | Periodic `refresh_reserves` | Ledger touched |
|--------|---------------------------|----------------|
| **phoenix** | Factory `query_all_pools_details` → updates all cached pairs | Same factory call; patches only touched pool addresses |
| **comet** | `refresh_pool` per known pool → updates `pairs` + weighted `pool_states` | `refresh_pool` for touched known addresses |

Both implement `get_cached_pairs()` so the worker 5s loop updates snapshots. Redis xy=k keys use the two-token edge reserves from the graph.

## Configuration (environment)

| Variable | Default | Meaning |
|----------|---------|---------|
| `POOL_STATE_TTL_SECS` | `30` | Redis EX for per-pool keys |
| `POOL_PUBLISH_INTERVAL_SECS` | `2` | Worker: parallel refresh + Redis publish cadence |
| `POOL_STATE_REFRESH_CONCURRENCY` | `4` | Worker: concurrent getLedgerEntries batches (xy=k) |
| `QUOTE_RPC_HYDRATE_ENABLED` | `false` | API: allow RPC on Redis miss (emergency only) |
| `QUOTE_HYDRATE_MAX_POOLS` | `12` | API: max xy=k RPC hydrates when enabled |
| `SNAPSHOT_REDIS_URL` | — | Required for pool state store (same Redis as snapshots) |

## Related code

- `crates/market-snapshot/src/pool_state_store.rs` — keys, TTL, publish/MGET, CLMM policy
- `crates/market-data-worker` — publishes pool keys after each snapshot
- `crates/api-server/src/pool_hydrate.rs` — batched quote hydration
- `crates/router-engine/src/quote_engine.rs` — `QuoteHydration` overlay for `get_route_with_paths`

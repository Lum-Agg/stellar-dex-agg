# Stellar DEX Aggregator (LumAgg)

[![GitHub](https://img.shields.io/badge/GitHub-ligulfzhou%2Fstellar--dex--agg-181717?logo=github)](https://github.com/Lum-Agg/stellar-dex-agg)
[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)

Multi-source liquidity aggregation for Stellar's Soroban DEX ecosystem.

**Repository:** https://github.com/Lum-Agg/stellar-dex-agg  
**中文文档:** [README.zh-CN.md](README.zh-CN.md)

LumAgg routes swaps across **Soroswap**, **Aquarius** (xy=k, stable, CLMM), **Phoenix**, **Sushi V3**, and **Comet**, with optional comparison against **Classic DEX** (Horizon PathPayment). It supports multi-hop paths, split orders across venues, and atomic on-chain execution via an optional aggregator contract.

## Contents

- [Architecture](#architecture)
- [DEX sources](#dex-sources)
- [Key features](#key-features)
- [Why not Classic DEX routing?](#why-not-classic-dex-routing)
- [Project structure](#project-structure)
- [Development](#development)
- [Deployment](#deployment)
- [Configuration](#configuration)
- [Split routing](#split-routing)
- [Related docs](#related-docs)
- [License](#license)

## Architecture

### Design principles

| Principle | Implementation |
|-----------|----------------|
| **Topology vs state** | Routing graph (pairs, pool IDs, fees) is separate from live reserves / ticks |
| **Single writer** | `market-data-worker` owns all Redis writes; `api-server` is stateless |
| **Event-driven freshness** | Ledger events refresh *touched* pools; no periodic full-market sweep |
| **Cold pools stay valid** | Pools with no on-chain activity keep their last Redis value until overwritten |

Pool state updates use three channels: **bootstrap** (worker start), **ledger watcher** (hot path, poll 0.1s), and **discovery** (~600s reconciliation). See [`docs/pool-state-architecture.md`](docs/pool-state-architecture.md) for details.

### System overview

Two paths share one Redis store.

**1 — Pool state writes (`market-data-worker`)**

```mermaid
flowchart LR
  subgraph redis [Redis]
    direction TB
    SNAP["Snapshot<br/>routing graph"]
    POOL["Pool state<br/>reserves / ticks"]
    PUB["Pub/Sub<br/>snapshot events"]
    SNAP --> POOL --> PUB
  end

  subgraph worker ["market-data-worker — single writer"]
    direction TB
    BD["Bootstrap + discovery<br/>~600s reconcile"]
    LW["Ledger watcher<br/>poll 0.1s · per-ledger getEvents"]
    FP["Fetch pipeline"]
    AD["DEX adapters"]

    LW -->|touched pools| FP
    FP --> AD
  end

  RPC[(Soroban RPC)]

  AD --> RPC
  LW --> RPC

  BD -->|publish| SNAP
  BD -->|publish| POOL
  BD -->|publish| PUB
  FP -->|refresh| POOL
```

**2 — Quote reads (`api-server`)**

```mermaid
flowchart LR
  FE[Frontend / SDK] -->|REST /quote /build_tx| API[api-server]

  subgraph RE [router-engine]
    PF[PathFinder<br/>BFS multi-hop]
    QE[QuoteEngine<br/>local AMM / CLMM math]
    SO[SplitOptimizer<br/>Brent method]
    PF --> QE --> SO
  end

  API --> RE
  API -->|MGET hydrate| POOL[(Pool state)]
  API -->|graph reload| SNAP[(Snapshot)]
  PUB[(Pub/Sub)] -.->|hot reload| API

  API -->|Classic benchmark| HZN[Horizon API]
  API -->|build XDR| AGG[Aggregator contract]
```

After path discovery and Redis hydration, **QuoteEngine** quotes each candidate path locally, then **SplitOptimizer** decides whether to send the full amount down one path or split across several. For two paths it uses **Brent's method** (~10 evaluations, ~0.01% tolerance) to find the optimal input ratio; for more paths it merges pairwise (recursive Brent for 2-path merges, output-weighted seed for 3+). Splitting runs only when price impact exceeds `SPLIT_THRESHOLD_BPS` or competing paths are within `SPLIT_COMPETITIVE_DELTA_BPS`. See [Split routing](#split-routing).

**Redis keys** (pool keys use `EX=86400`):

| Key pattern | Contents |
|-------------|----------|
| `lumagg:snapshot:*` | Versioned graph + CLMM metadata (no reserves) |
| `lumagg:pool:xyk:{source}:{pool}` | xy=k reserves |
| `lumagg:pool:aquarius:{pool}` | Aquarius N-token / stable reserves |
| `lumagg:pool:comet:{pool}` | Comet weighted pool (token balances + weights + fee) |
| `lumagg:pool:clmm:{source}:{pool}` | CLMM slot0, liquidity, ticks |
| `lumagg:snapshot:events` | Pub/Sub channel for snapshot hot-reload |

### Data layers

| Layer | Contents | Storage | Update cadence |
|-------|----------|---------|----------------|
| **Graph** | Token pairs, pool addresses, fee tiers; CLMM refs (no ticks) | `lumagg:snapshot:*` | Bootstrap; discovery ~600s |
| **Pool state** | xy=k reserves; Aquarius multi-token; Comet weighted; CLMM slot0 + ticks + coverage | `lumagg:pool:*` | Ledger poll 0.1s for touched pools; bootstrap + discovery full publish |

The API does **not** keep a long-lived in-process pool cache. Each `/quote` reloads the graph from the last snapshot, then overlays pool state from Redis (`QUOTE_RPC_HYDRATE_ENABLED=false` by default).

### Quote request flow

```mermaid
sequenceDiagram
  participant C as Client
  participant API as api-server
  participant PF as PathFinder
  participant R as Redis
  participant QE as QuoteEngine
  participant SO as SplitOptimizer

  C->>API: GET /api/v1/quote
  API->>PF: find_candidate_paths (in-memory graph)
  PF-->>API: candidate paths
  API->>R: MGET pool keys (xyk + aquarius + clmm + comet)
  R-->>API: cached pool states
  API->>QE: quote each path at full amount
  QE-->>API: QuotedPath list
  API->>SO: optimize (Brent if split warranted)
  Note over SO: 2 paths: Brent ratio<br/>N paths: pairwise merge
  SO-->>API: OptimalRoute (single or split)
  API-->>C: quote + pool_addresses
```

Steps in code:

1. **Path discovery** — BFS on the routing graph; all candidate paths (no liquidity prune).
2. **Collect pool keys** — unique `(source, pool_address)` across paths.
3. **Hydrate pool state** (`pool_hydrate::hydrate_paths`) — Redis MGET for xy=k, Aquarius, CLMM, and Comet weighted state (written by worker). Optional Soroswap xy=k / Comet RPC fallback for Redis misses when `QUOTE_RPC_HYDRATE_ENABLED=true`.
4. **Per-path quote** — local AMM / CLMM / Comet math at full `amount_in`; skip CLMM hops when `coverage.is_complete` is false.
5. **Split optimization** — `SplitOptimizer`: skip if impact below threshold; else Brent's method (2-path) or pairwise merge (N-path) to maximize total output.
6. **Classic compare** — optional Horizon PathPayment benchmark vs best Soroban route.

### Ledger watcher (hot path)

Stellar Soroban RPC has no WebSocket / Geyser push — the worker **polls** `getLatestLedger` every **0.1s**, then fetches **one ledger at a time**:

```text
getLatestLedger
  → for each new ledger N:
      getEvents(N, N+1)
      → match contractId to KnownPoolIndex (pool + router event parse)
      → fetch pipeline: RPC refresh touched pools only
      → Redis SET (CLMM only if coverage.is_complete)
```

Active pools typically refresh within **~0.1–2s** after a swap / add / remove on-chain.

**CLMM policy:** tick data lives in pool contract storage. Worker writes Redis only when `coverage.is_complete`; otherwise quote engine skips those hops.

## DEX sources

| Source | Pool type | In routing graph | Pool state in Redis | Quote math |
|--------|-----------|------------------|---------------------|------------|
| **soroswap** | xy=k | Yes | Discovery + ledger | Constant product |
| **aquarius** | xy=k + stable | Yes | Discovery + ledger | xy=k / stable |
| **phoenix** | xy=k | Yes | Discovery + ledger | Fee-on-output |
| **sushi** | CLMM V3 | Yes | Discovery + ledger | Local `clmm_math` |
| **aquarius_clmm** | CLMM | Yes | Discovery + ledger | Local `clmm_math` |
| **comet** | Weighted | Yes | Discovery + ledger (`lumagg:pool:comet:*`) | Balancer math |
| **classic_dex** | Native orderbook | **No** | **Per quote** (Horizon) | Benchmark only |

## Key features

- **Multi-source aggregation** across six Soroban DEX families
- **Multi-hop routing** — BFS through intermediate tokens (configurable max hops)
- **Split orders** — Brent optimizer across paths when impact or competitiveness warrants it
- **Event-driven pool state** — ledger watcher + discovery; no periodic full sweep
- **Horizontally scalable API** — stateless `api-server` instances behind a load balancer
- **Sub-2s hot pool freshness** — 0.1s ledger poll + fetch pipeline for touched pools
- **Atomic on-chain execution** — optional aggregator contract (`split_swap`, `round_trip_swap`)
- **Classic benchmark** — Horizon PathPayment per quote without polluting the Soroban graph

## Why not Classic DEX routing?

Stellar's native PathPayment has **uncontrollable routing** — Stellar Core decides how to split across orderbooks and pools. You cannot force a specific pool or path.

This aggregator targets **Soroban DEXes** where each hop is a deterministic contract call. Classic DEX is a **per-quote benchmark** (and fallback for tokens only on the native DEX), not an edge in the pathfinder graph.

## Project structure

```
├── contracts/aggregator/       # Soroban contract (split_swap, round_trip_swap)
├── crates/
│   ├── market-snapshot/        # MarketSnapshot schema, Redis pool_state_store
│   ├── market-data-worker/     # Discovery, ledger watcher, fetch pipeline
│   ├── dex-adapters/           # Per-DEX adapters, RPC, pool index, router events
│   ├── router-engine/          # PathFinder, QuoteEngine, split optimizer
│   ├── api-server/             # REST API (/quote, /build_tx, /tokens)
│   ├── arbitrage/              # Round-trip arb scanner (aggregator.round_trip_swap)
│   ├── analytics-indexer/      # On-chain aggregator tx indexer (SCF analytics v0)
│   ├── lumagg-alerts/          # Telegram / monitoring alerts
│   └── sdk/                    # Client SDK
├── docs/
│   ├── pool-state-architecture.md
│   └── analytics-indexer.md    # Volume attribution spec + indexer ops
├── thirdparty/                 # Optional local upstream clones (not tracked; see README there)
├── deploy/                     # systemd units (lumagg-api@, lumagg-worker)
├── deploy_server.sh            # rsync + build + restart on remote host
└── frontend/                   # SvelteKit demo UI
```

## Third-party reference

Upstream DEX repos are **not** vendored in git. For contract layout / mainnet manifests when editing adapters, clone into `thirdparty/` — see [thirdparty/README.md](./thirdparty/README.md). Builds and deploy do not require it.

## Development

```bash
# Compile
cargo check --workspace --exclude aggregator-contract

# Tests
cargo test --workspace --exclude aggregator-contract

# Local file-backed stack (dev)
SNAPSHOT_DIR=data/snapshots cargo run -p market-data-worker
SNAPSHOT_DIR=data/snapshots cargo run -p api-server

# Local Redis-backed stack (production-style)
redis-server --port 6380 --save "" --appendonly no

SNAPSHOT_BACKEND=redis \
SNAPSHOT_REDIS_URL=redis://127.0.0.1:6380/ \
SNAPSHOT_REDIS_CHANNEL=lumagg:snapshot:events \
cargo run -p market-data-worker

LISTEN_ADDR=127.0.0.1:3113 \
SNAPSHOT_BACKEND=redis \
SNAPSHOT_REDIS_URL=redis://127.0.0.1:6380/ \
SNAPSHOT_REDIS_CHANNEL=lumagg:snapshot:events \
cargo run -p api-server
```

**Utility binaries** (`dex-adapters`):

```bash
# Audit ledger events → touched pools (live chain)
RPC_URL=... REDIS_URL=... AUDIT_LEDGERS=30 \
  cargo run -p dex-adapters --release --bin audit-ledger-events

# Dump per-ledger events to JSONL
DUMP_DIR=./ledger-events-dump DUMP_LEDGERS=5 \
  cargo run -p dex-adapters --release --bin dump-ledger-events
```

## Deployment

```bash
./deploy_server.sh          # api-server (4 instances) + worker
./deploy_server.sh api      # api-server only
./deploy_server.sh worker   # market-data-worker only
```

Systemd units live in `deploy/`:

| Unit | Role |
|------|------|
| `lumagg-worker.service` | Single writer — discovery, ledger watcher, Redis publish |
| `lumagg-api@.service` | Stateless API instances (ports 3100–3103) |

Worker defaults include `LEDGER_POLL_SECS=0.1`, `FETCH_PIPELINE_ENABLED=true`, `DISCOVERY_INTERVAL_SECS=600`.

## Configuration

### Shared

| Variable | Default | Meaning |
|----------|---------|---------|
| `RPC_URL` | mainnet gateway.fm | Soroban JSON-RPC endpoint |

### Snapshot & Redis

| Variable | Default | Component | Meaning |
|----------|---------|-----------|---------|
| `SNAPSHOT_BACKEND` | — | worker, API | `file` or `redis` |
| `SNAPSHOT_REDIS_URL` | — | worker, API | Redis URL (snapshots + pool state) |
| `SNAPSHOT_REDIS_CHANNEL` | `lumagg:snapshot:events` | worker, API | Pub/Sub for snapshot hot-reload |
| `SNAPSHOT_POLL_INTERVAL_MS` | `1000` | API | Polling fallback if Pub/Sub missed |
| `POOL_STATE_TTL_SECS` | `86400` | worker | Redis EX on pool keys (eviction, not freshness SLA) |

### Worker — discovery & ledger

| Variable | Default | Meaning |
|----------|---------|---------|
| `DISCOVERY_INTERVAL_SECS` | `600` | Full graph rediscovery + Redis pool publish |
| `REFRESH_INTERVAL_SECS` | `5` | Adapter cache refresh (feeds discovery) |
| `LEDGER_WATCHER_ENABLED` | `true` | Requires Redis pool store |
| `LEDGER_POLL_SECS` | `0.1` | Ledger poll interval (min `0.1`) |
| `LEDGER_MAX_CATCHUP` | `32` | Max ledgers ingested per poll after backlog |
| `FETCH_PIPELINE_ENABLED` | `true` | Ledger touched → task queue → Redis |
| `FETCH_WORKER_COUNT` | `8` | Concurrent RPC workers in fetch pipeline |
| `POOL_STATE_REFRESH_CONCURRENCY` | `8` | Concurrent getLedgerEntries batches |

### API — quoting & routing

| Variable | Default | Meaning |
|----------|---------|---------|
| `QUOTE_RPC_HYDRATE_ENABLED` | `false` | RPC fallback on Redis miss (emergency) |
| `QUOTE_HYDRATE_MAX_POOLS` | `12` | Max xy=k RPC hydrates when enabled |
| `PATH_FINDER_MAX_HOPS` | `3` | Max hops per path |
| `PATH_FINDER_MAX_MULTI_HOP_PATHS` | `50` | Cap on 2+ hop paths per quote |
| `PATH_FINDER_MAX_DIRECT_PATHS` | `0` | Cap on 1-hop pools (`0` = all) |
| `MAX_SPLITS` | `5` | Max candidate paths for split optimization |
| `SPLIT_THRESHOLD_BPS` | `5` | Min price impact (bps) before split attempt |
| `SPLIT_COMPETITIVE_DELTA_BPS` | `50` | Also split when 2nd path is within this bps of best |
| `MIN_SPLIT_FRACTION_BPS` | `5` | Drop split legs below this output share |

### DEX discovery overrides

| Variable | Default | Meaning |
|----------|---------|---------|
| `SUSHI_DISCOVERY_RPC` | public gateway | RPC for Sushi pool probes |
| `COMET_FACTORY` | Blend mainnet factory | Comet factory contract |
| `COMET_EXTRA_POOLS` | — | Comma-separated extra Comet pool IDs |

## Split routing

Production `deploy/lumagg-api@.service` sets split env vars explicitly.

The **SplitOptimizer** (`crates/router-engine/src/split_optimizer.rs`) runs inside QuoteEngine after every path has been quoted at full size:

| Case | Algorithm |
|------|-----------|
| Impact &lt; `SPLIT_THRESHOLD_BPS` and paths not competitive | Single best path (no split) |
| 2 paths | **Brent's method** on `[0, 1]` to maximize `out_a(x) + out_b(1−x)` |
| 3+ paths | Pairwise recursive Brent merges; 3+ seed uses output-weighted allocation |

Brent tolerance defaults to `0.0001` (0.01%) with up to 18 iterations — similar in spirit to Jupiter Iris (golden-section + Brent).

- **`SPLIT_THRESHOLD_BPS=5` (0.05%)** — split runs when estimated impact ≥ 5 bps, or paths are competitive (within `SPLIT_COMPETITIVE_DELTA_BPS` with impact > 0).
- **`SPLIT_THRESHOLD_BPS=1` (0.01%)** — usually not worth it: more optimizer work, many quotes still return `split_rejected_reason: "no_improvement"`.
- Use `?debug=1` on `/quote` to see `split_attempted`, `split_threshold_bps`, `split_rejected_reason`, and `split_method` (e.g. `two_path_brent`).

## Related docs

| Doc | Topic |
|-----|-------|
| [`docs/analytics-indexer.md`](docs/analytics-indexer.md) | On-chain analytics indexer v0 — attribution spec, env, export |
| [`docs/pool-state-architecture.md`](docs/pool-state-architecture.md) | Pool state design, env tables, code pointers |
| [`docs/scf-venue-comparison.md`](docs/scf-venue-comparison.md) | LumAgg vs Soroswap / Stellar Broker — venue coverage & SCF differentiation evidence |
| [`docs/scf-resubmission-budget.md`](docs/scf-resubmission-budget.md) | SCF #44 resubmission — $80k tranche deliverables (copy-paste) |
| [`docs/arb-executor.md`](docs/arb-executor.md) | Atomic arb operator stack (vault + `round_trip_swap` bot) |

## License

Apache-2.0. See [LICENSE](LICENSE).

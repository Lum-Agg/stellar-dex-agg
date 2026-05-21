# Stellar DEX Aggregator

Multi-source liquidity aggregation router for Stellar's Soroban DEX ecosystem.

Aggregates liquidity across **Soroswap**, **Aquarius**, **Phoenix**, and other Soroban DEXes to find optimal swap execution — including split orders across multiple paths.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (SvelteKit)                   │
└─────────────────────┬───────────────────────────────────┘
                      │ REST API
┌─────────────────────▼───────────────────────────────────┐
│                  API Server (Axum)                        │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│               Router Engine (Rust)                        │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │PathFinder│  │SplitOptimizer│  │TransactionBuilder│  │
│  │  (BFS)   │  │  (Greedy)    │  │  (XDR gen)       │  │
│  └────┬─────┘  └──────┬───────┘  └────────┬─────────┘  │
└───────┼────────────────┼───────────────────┼────────────┘
        │                │                   │
┌───────▼────────────────▼───────────────────▼────────────┐
│              DEX Adapters (Rust)                          │
│  ┌─────────┐  ┌─────────┐  ┌───────┐  ┌───────────┐   │
│  │Soroswap │  │Aquarius │  │Phoenix│  │Classic DEX│   │
│  │(xy=k)   │  │(xy=k +  │  │(fee on│  │(benchmark)│   │
│  │         │  │ stable)  │  │output)│  │           │   │
│  └─────────┘  └─────────┘  └───────┘  └───────────┘   │
└─────────────────────────┬───────────────────────────────┘
                          │ Soroban RPC
┌─────────────────────────▼───────────────────────────────┐
│              Stellar Network (Soroban)                    │
│  ┌──────────────────────────────────────────────────┐   │
│  │         Aggregator Contract (split_swap)          │   │
│  │  Executes multi-path swaps atomically             │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Key Features

- **Multi-source aggregation**: Finds best price across Soroswap, Aquarius, Phoenix
- **Split orders**: Automatically splits large trades across multiple DEXes to reduce price impact
- **Multi-hop routing**: BFS path discovery through intermediate tokens (up to 4 hops)
- **Atomic execution**: On-chain aggregator contract ensures all-or-nothing execution
- **Slippage protection**: User-configurable minimum output guarantee

## Why not Classic DEX?

Stellar's native PathPayment has **uncontrollable routing** — Stellar Core decides how to split across orderbooks and liquidity pools. You can't force a specific execution path.

Our aggregator targets **Soroban DEXes** where each swap is a deterministic contract call with predictable output. This is liquidity that Stellar Core's native routing cannot reach.

## Project Structure

```
├── contracts/aggregator/     # Soroban smart contract (atomic swap execution)
├── crates/
│   ├── market-snapshot/      # Shared serialized market state + file/Redis snapshot store helpers
│   ├── market-data-worker/   # Background snapshot publisher
│   ├── dex-adapters/         # DEX protocol adapters + Soroban RPC client
│   ├── router-engine/        # Path finding, quoting, split optimization
│   ├── api-server/           # REST API (Axum)
│   └── sdk/                  # Client SDK
└── frontend/                 # SvelteKit demo (TODO)
```

## Development

```bash
# Check compilation
cargo check --workspace --exclude aggregator-contract

# Run tests
cargo test --workspace --exclude aggregator-contract

# Run API server
cargo run -p api-server

# Run file-backed market snapshot worker
SNAPSHOT_DIR=data/snapshots cargo run -p market-data-worker

# Run API server from snapshots instead of in-process adapters
SNAPSHOT_DIR=data/snapshots cargo run -p api-server

# Start a local Redis for shared snapshot mode
redis-server --port 6380 --save "" --appendonly no

# Run Redis-backed market snapshot worker
SNAPSHOT_BACKEND=redis \
SNAPSHOT_REDIS_URL=redis://127.0.0.1:6380/ \
SNAPSHOT_REDIS_CHANNEL=lumagg:snapshot:events \
SNAPSHOT_REDIS_KEEP_LATEST=3 \
cargo run -p market-data-worker

# Run API server from Redis-backed snapshots
LISTEN_ADDR=127.0.0.1:3113 \
SNAPSHOT_BACKEND=redis \
SNAPSHOT_REDIS_URL=redis://127.0.0.1:6380/ \
SNAPSHOT_REDIS_CHANNEL=lumagg:snapshot:events \
SNAPSHOT_POLL_INTERVAL_MS=250 \
cargo run -p api-server --bin api-server
```

## Redis Snapshot Mode

Redis-backed snapshot mode is the recommended path for multi-instance API deployment behind Nginx.

- `market-data-worker` is the only writer. It publishes versioned snapshot payloads into Redis.
- `api-server` instances stay stateless. They load from Redis, hot-reload on Pub/Sub events, and fall back to polling `lumagg:snapshot:current` if an event is missed.
- Old Redis snapshot versions are pruned automatically; `SNAPSHOT_REDIS_KEEP_LATEST` controls how many versions remain.

Relevant environment variables:

- `SNAPSHOT_BACKEND=file|redis`
- `SNAPSHOT_REDIS_URL=redis://host:port/`
- `SNAPSHOT_REDIS_CHANNEL=lumagg:snapshot:events`
- `SNAPSHOT_REDIS_KEEP_LATEST=3`
- `SNAPSHOT_POLL_INTERVAL_MS=250`
- `SNAPSHOT_DIR=data/snapshots` for file-backed mode only

## License

MIT

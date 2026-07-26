# Analytics Indexer (v0)

On-chain analytics for the LumAgg **aggregator contract** — Tranche 1 Deliverable 4.

## Scope

The indexer polls Soroban RPC **`getEvents`** for the aggregator contract, groups `swap` / `rt` / `leg` topics per `tx_hash`, and stores invocations in SQLite. Daily rollups export as JSON for Tranche 3 dashboard wiring.

**Vault path:** When arb uses `vault.execute_round_trip`, the aggregator still emits events via CPI — same indexer path as direct `round_trip_swap`.

**Legacy fallback:** `INDEXER_ENVELOPE_FALLBACK=1` (default with `INDEXER_MODE=events`) also ingests pre-upgrade txs and supplies token/path metadata for historical 4-field `leg` events. New compact `leg` events are self-contained.

## Aggregator events (requires WASM upgrade)

After upgrading mainnet aggregator WASM, each successful invoke emits:

| Topic | When | Data fields |
|-------|------|-------------|
| `swap` | `swap()` completes | user, token_in, token_out, amount_in, amount_out, route_count |
| `rt` | `round_trip_swap()` completes | user, base, bridge, amount_in, amount_out, serial_depth, is_split |
| `leg` | each DEX hop | leg_index, dex_tag, pool, token_in, actual amount_in |

`dex_tag`: 0=aquarius, 1=soroswap, 2=phoenix, 3=sushi, 4=comet

Upgrade: `./contracts/aggregator/upgrade.sh` (see repo README).

## Architecture

```mermaid
flowchart LR
  RPC[Soroban RPC getEvents] --> IDX[analytics-indexer]
  ENV[optional envelope fallback] --> IDX
  IDX --> DB[(SQLite)]
  IDX --> EXP[export-daily JSON]
  EXP --> T3[Tranche 3 dashboard / API]
```

1. **Ingest loop** — advance ledger cursor in batches (≤10k ledgers per RPC call).
2. **Parse events** — decode topic + value XDR; group legs by `tx_hash`.
3. **Store** — `swap_invocations` + `swap_legs`; idempotent on `tx_hash`.
4. **Export** — aggregate by UTC day.

## Volume attribution spec

| Field | Source | Notes |
|-------|--------|-------|
| **Function** | topic `swap` → `swap`; topic `rt` → `round_trip_swap` | |
| **User** | summary event field 0 | G-address or contract |
| **Entry notional** | summary event | Invocation input, grouped by entry token |
| **Routed volume** | `leg` event + envelope token metadata | Sum of each executed leg's actual input, grouped and priced by that leg's token |
| **Split swap** | `route_count > 1` on swap events; `is_split` on round-trip events | |
| **DEX attribution** | `leg` events | Successful, actually executed hops only |
| **Pool** | `leg` event pool address | |
| **Status** | `inSuccessfulContractCall` | default SUCCESS |

`by_token` remains the stable entry-notional breakdown consumed by DefiLlama.
Per-leg routed amounts use the separate `routed_by_token` field so intermediate
hop tokens cannot change external volume semantics.

## Round-trip surplus

For each successful `round_trip_swap`, the indexer derives:

`gross_surplus = amount_out - amount_in`

This is an on-chain execution result, grouped by base token and optionally priced
to historical daily USD by the API. It is **gross surplus, not net P&L**:
transaction fees are not present in aggregator events and are not deducted.
Failed transactions and bot simulation estimates are excluded.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `INDEXER_MODE` | `events` | `events` \| `envelope` \| `both` |
| `INDEXER_ENVELOPE_FALLBACK` | `true` when mode=events | Backfill legacy invokes and enrich historical 4-field leg events; optional after deploying compact events |
| `INDEXER_RPC_URL` / `SOROBAN_RPC_URL` | mainnet gateway.fm | Soroban RPC endpoint |
| `AGGREGATOR_CONTRACT` | mainnet LumAgg aggregator | Event source contract |
| `INDEXER_DB_PATH` | `./data/analytics-indexer.db` | SQLite file |
| `INDEXER_POLL_SECS` | `30` | Poll interval |
| `INDEXER_START_LEDGER` | — | Initial ledger (else latest − 17,280) |
| `INDEXER_PAGE_LIMIT` | `10000` | getEvents page size |

## Commands

```bash
# Continuous ingest (production)
cargo run -p analytics-indexer -- run

# One-shot backfill from ledger
INDEXER_START_LEDGER=63200000 cargo run -p analytics-indexer -- backfill

# Status
cargo run -p analytics-indexer -- status

# Daily JSON export
cargo run -p analytics-indexer -- export-daily
cargo run -p analytics-indexer -- export-daily 2026-06-01
```

## Tranche 3 handoff

- `export-daily` maps to planned dashboard cards.
- **`GET /api/v1/stats`** on api-server when `INDEXER_DB_PATH` is set (same DB file).
- Public UI: https://lumagg.xyz/stats
- Sample export: [sample-indexer-export.json](./sample-indexer-export.json)

```bash
# api-server env (alongside indexer)
INDEXER_DB_PATH=/opt/stellar-dex-aggregator/data/analytics-indexer.db
curl -s https://api.lumagg.xyz/api/v1/stats | jq .
```

## Development

The development schema currently has no migration layer. After schema changes,
start from a fresh indexer SQLite file (back up any data you need first) and
backfill aggregator events.

For the compact `leg` rollout:

1. Upgrade the Aggregator WASM before disabling envelope fallback.
2. Backfill pre-upgrade ledgers with `INDEXER_ENVELOPE_FALLBACK=1`.
3. Run the live indexer with `INDEXER_ENVELOPE_FALLBACK=0`; new events already
   contain the input token and actual execution input.

```bash
cargo test -p analytics-indexer
cargo test -p aggregator-contract   # event emission in contract tests
```

Crate layout: `crates/analytics-indexer/` · RPC: `crates/dex-adapters/src/rpc/events.rs`

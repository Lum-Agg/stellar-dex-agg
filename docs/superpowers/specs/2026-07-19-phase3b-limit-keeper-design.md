# LumAgg Phase 3b: Limit Keeper (Lean MVP)

**Date:** 2026-07-19  
**Status:** Approved for planning  
**Depends on:** `contracts/order-escrow` (3a), LumAgg `GET /quote`

## Goal

Run a self-hosted **limit-keeper** that discovers open Limit orders via Soroban **getEvents**, quotes via LumAgg API, and permissionlessly calls `escrow.fill` when the limit is met. No DCA, no retail API/UI, no fill incentives.

## Decisions

| Topic | Choice |
|-------|--------|
| Scope | Lean keeper only |
| Discovery | RPC `getEvents` (not storage scan / not env ID list) |
| Code layout | New crate `crates/limit-keeper` |
| Escrow events | **Must add** `order_created` / `order_cancelled` / `order_expired` (create currently silent; only `order_filled` exists) |
| DCA | Out of scope |
| REST / UI | Out of scope (3c / 3d) |

## Escrow event additions (contract tweak in 3b)

| Topic symbol | When | Suggested data |
|--------------|------|----------------|
| `order_created` | end of `create_limit` | `(owner, token_in, token_out, amount_in, limit_out_per_in_e7, expires_ledger)` + topic includes `order_id` like fill |
| `order_cancelled` | end of `cancel` | `(owner, refunded_amount)` + `order_id` topic |
| `order_expired` | end of `reclaim_expired` | `(owner, refunded_amount)` + `order_id` topic |
| `order_filled` | already present | keep |

Match existing fill style:

```rust
env.events().publish(
    (Symbol::new(&env, "order_filled"), order_id),
    (order.owner, amount_in, amount_out, order.amount_in_remaining),
);
```

## Keeper architecture

```
loop every POLL_SECS:
  getEvents(escrow, cursor..latest)
  apply to OpenOrderBook (in-memory + optional sqlite cursor file)
  for each Open order:
    if expired -> optional reclaim_expired (MVP: skip or reclaim; prefer reclaim if KEPER_RECLAIM=1)
    quote(token_in, token_out, amount_in=remaining or MAX_FILL)
    if expected_out < required_min_out(amount, limit_out_per_in_e7): continue
    build fill invoke with sub_routes from quote
    simulateTransaction -> sign -> sendTransaction
```

**Limit math (same as contract):**

```text
required_min_out = floor(amount_in * limit_out_per_in_e7 / 10_000_000)
```

Use quote `expected_output` (and submit `min_amount_out = required_min_out`, not looser than contract). Optionally set `min_amount_out = max(required_min_out, quote.minimum_output)` so slippage protects filler sim.

## Components (`crates/limit-keeper`)

| Module | Role |
|--------|------|
| `config` | Env parse |
| `events` | Parse escrow contract events via `dex_adapters` RPC helpers (reuse indexer patterns) |
| `book` | In-memory open orders keyed by `order_id` |
| `limit` | `required_min_out` pure fn + tests |
| `quote` | Thin HTTP client (can copy/adapt `arbitrage::quote_client` or call API directly) |
| `execute` | Build `fill` HostFunction, prepare/sim/sign/submit (mirror arb prepare/submit patterns; **do not** import vault round-trip) |
| `main` | Poll loop + tracing |

## Config (env)

| Var | Purpose |
|-----|---------|
| `KEEPER_RPC_URL` | Soroban RPC |
| `KEEPER_SECRET` | Filler key seed/secret |
| `KEEPER_NETWORK` | public/testnet passphrase |
| `ESCROW_CONTRACT` | C… |
| `AGGREGATOR_CONTRACT` | C… (needed if building routes / validation) |
| `QUOTE_API_URL` | e.g. `https://api.lumagg.xyz` |
| `KEEPER_POLL_SECS` | default 10 |
| `KEEPER_CURSOR_PATH` | file for last processed ledger |
| `KEEPER_DRY_RUN` | `1` = quote+log only, no submit |
| `KEEPER_MAX_FILL` | optional cap per fill (stroops) |
| `KEEPER_RECLAIM` | `1` = auto reclaim expired |

## Non-goals

- DCA fills  
- Order create/cancel HTTP API  
- Frontend Limit tab  
- Multi-keeper coordination / mempool races beyond best-effort  
- Fill tip economics  

## Testing & acceptance

**Automated**
- Contract tests: events emitted on create/cancel/reclaim  
- Keeper unit: event parser + `required_min_out`  
- Keeper: dry-run path never submits when `KEEPER_DRY_RUN=1`

**Manual / integration**
- Deploy escrow (testnet or local), create limit with executable rate, run keeper with dry-run off → fill succeeds  
- Too-tight limit → no submit  

## Acceptance

1. Open orders discovered solely from events after create  
2. Executable limits get filled by keeper  
3. Non-executable limits are skipped without crash  
4. No DCA/UI/API surface shipped  

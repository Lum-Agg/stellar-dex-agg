# LumAgg Phase 3: Limit + DCA Architecture Research

**Date:** 2026-07-19  
**Status:** Research / architecture only — **no implementation in this phase**  
**Depends on:** Aggregator `swap`, quote API, Phase 1–2 retail surfaces (orders UI later)

## Goal

Decide how LumAgg adds **limit orders** and **DCA** on Stellar/Soroban without bloating the arb vault or rewriting the aggregator. This document locks product/architecture defaults and an implementation roadmap for follow-on specs (`3a` contract, `3b` keeper, …).

## Decisions locked in research

| Topic | Decision |
|-------|----------|
| Research first | Spec only; no contracts/keepers shipped from this doc |
| Custody default | **Escrow contract** (`order-escrow`) |
| Allowance pull | Documented alternative; **not** MVP default |
| Limit semantics | Exact-in + **min out / limit price** → fill via `aggregator.swap` |
| DCA | Same escrow, chunked timed fills |
| Fill auth | **Permissionless** `fill` |
| Fill incentive (MVP) | **None** — LumAgg self-run bot calls open interface; tips/rebate later |
| Arb vault reuse | **No** for retail user funds |

## Non-goals (this research + early MVP slices)

- Classic SDEX-style bilateral order book UX
- TWAP grids / auction-style fill markets
- Fill rebates, spread-sharing, or MEV auctions (post-MVP)
- Centralized custodian / off-chain balance ledger
- Replacing aggregator market swap path

## Why a new stack is required

Soroban has **no native price/time triggers**. Immediate `aggregator.swap` cannot “sit” until a price hits. Any limit/DCA needs:

1. Locked or pullable inventory (escrow or allowance)
2. An off-chain (or permissionless) **filler** that submits a tx when conditions hold
3. On-chain checks that the fill respects the user’s limit / schedule

The existing **arb vault** is an ops float with allowlisted callers and round-trip semantics — wrong trust model for retail. Lessons *to reuse* from vault/arb code: client-passed allowance expiry, fixed-ceiling approve patterns, quote→simulate→submit loop — not the vault contract itself.

## Approach comparison

### A — Escrow + permissionless `fill` (**default**)

```
User --create_limit--> Escrow (locks token_in)
Anyone --fill(order_id, sub_routes, min_out)--> Escrow
                                              |--check limit / schedule-->
                                              |--CPI aggregator.swap-->
                                              '--send token_out to owner
User --cancel--> Escrow refunds remaining
```

**Pros:** Clear custody audit trail; cancel/expire/partial fill natural; filler cannot steal locked principal beyond swap rules.  
**Cons:** Extra deposit step; escrow holds assets (TTL/rent ops).

### B — Allowance pull

User keeps funds; `approve(escrow_or_filler, amount)`; fill does `transfer_from` + swap.

**Pros:** Familiar “approve once” UX.  
**Cons:** Allowance TTL/refresh; max-approval hazards; harder partial cancel story; permissionless filler attack surface larger if mis-scoped.

### C — Centralized custody

Rejected: contradicts non-custodial aggregator positioning.

## Escrow contract sketch (`contracts/order-escrow`)

Conceptual only — field names may change in `3a` implementation plan.

### Limit order fields

| Field | Purpose |
|-------|---------|
| `owner` | Order creator |
| `token_in` / `token_out` | Pair |
| `amount_in_remaining` | Unfilled sell amount |
| `min_out_bps_of_in` **or** stored `limit_rate` | Limit condition (pick one encoding in `3a`; see below) |
| `expires_ledger` | Hard expiry |
| `status` | Open / Filled / Cancelled / Expired |

**Recommended limit encoding (MVP):** store `limit_rate` as “minimum `token_out` stroops per 1e7 `token_in` stroops” (or Q64-style rational). On fill of size `x`, require `min_amount_out >= floor(x * limit_rate / 1e7)`. Avoid freezing a single `min_amount_out` for the full size only — that breaks partial fills.

### Entrypoints (draft)

| Fn | Auth | Behavior |
|----|------|----------|
| `create_limit(...)` | owner | Transfer `amount_in` into escrow; emit `OrderCreated` |
| `create_dca(...)` | owner | Same + schedule fields |
| `cancel(order_id)` | owner | Refund `amount_in_remaining` |
| `fill(order_id, sub_routes, amount_in, min_amount_out)` | **anyone** | Validate open/not expired/limit or schedule; CPI `aggregator.swap` with escrow as `user` or as authorized puller; pay `token_out` to `owner`; reduce remaining |
| `reclaim_expired(order_id)` | anyone or owner | Refund after expiry |

Aggregator today: **user auth on `swap`**. Escrow fill must either:

1. Escrow calls aggregator as the **authorized invoker** with invoker auth / contract-as-user pattern supported by current aggregator, or  
2. Extend aggregator with a trusted “swap_from(payer)” — **prefer (1)** without aggregator changes if invoker auth already allows; **verify in `3a` spike** before freezing ABI.

> **Open spike (must resolve in 3a):** Confirm whether escrow can act as `swap`’s `user` Address with SAC pulls from escrow balance. If not, minimal aggregator extension is in scope for `3a` only.

### DCA fields (additional)

| Field | Purpose |
|-------|---------|
| `chunk_amount` | Max/exact size per fill |
| `interval_ledgers` | Spacing between fills |
| `next_executable_ledger` | Earliest next fill |
| `optional limit_rate` | Cap slippage per chunk; `None` = market chunk |

## Keeper / off-chain

**Component:** `limit-keeper` (name TBD) — thin cousin of `crates/arbitrage`.

**Loop:**

1. Load open orders (RPC events / indexer DB / contract storage scan — prefer indexer after first deploy)
2. For each candidate: `GET /quote` for `token_in→token_out` at intended size
3. Limit: executable iff `expected_out >= required_min_out(size)`  
   DCA: executable iff `ledger >= next_executable` and remaining ≥ chunk
4. Simulate `fill` XDR; sign with keeper key; submit
5. Pay resource fees from keeper account (no user tip in MVP)

**Permissionless:** any third party may run the same path. MVP ops: LumAgg runs at least one keeper for liveness; incentives deferred.

**Reuse from arb:** prepare/simulate/submit, fee gates, Telegram failure alerts — not vault round-trip logic.

## API / indexer / UI (later slices)

| Slice | Deliverable |
|-------|-------------|
| Indexer | Parse escrow events → `orders` / `fills` tables |
| API | `POST /orders`, `DELETE /orders/{id}`, `GET /orders?user=` |
| SDK | create/cancel/list helpers |
| UI | Limit + DCA tabs beside Swap; open orders list |

## Risks

| Risk | Mitigation |
|------|------------|
| Filler griefing / bad routes | On-chain min_out check; failed sim costs filler only |
| Price gap between quote and fill | Slippage in fill `min_amount_out`; keeper re-quotes often |
| Escrow rent / TTL | External keeper renews code, instance, and persistent entries; ops runbook |
| Aggregator auth incompatibility | 3a spike before full build |
| Partial fill dust | Min fill size; cancel dust threshold |
| No filler liveness | Self-run keeper SLA; later incentives |

## Testing & acceptance (for later implementation specs)

**Research acceptance (this doc):**  

- [x] Custody default = escrow with allowance comparison  
- [x] Permissionless fill + no MVP incentive  
- [x] Limit = min-rate market fill via aggregator  
- [x] DCA = scheduled chunks on same contract  
- [x] Roadmap slices 3a–3d  

**Future 3a acceptance (preview):** escrow create/cancel/fill on testnet against aggregator; unit tests for limit math and expiry.

## Implementation roadmap

| ID | Slice | Output |
|----|-------|--------|
| **3a** | Escrow contract + aggregator auth spike | Deployable testnet escrow; fill→swap proven |
| **3b** | limit-keeper MVP | Self-run bot fills open limits/DCA |
| **3c** | Indexer + order APIs | List/create/cancel for wallets |
| **3d** | Frontend Limit + DCA | Retail UX |
| **3e** | Incentives (optional) | Tips/rebate if permissionless competition needed |

Each slice gets its own design+plan when started. **Do not start 3b UI before 3a fill path works.**

## File map (future)

| Area | Likely path |
|------|-------------|
| Escrow | `contracts/order-escrow/` |
| Types | `contracts/shared-types/` if needed |
| Keeper | `crates/limit-keeper/` or extend `arbitrage` carefully behind feature flags |
| Indexer | extend `analytics-indexer` |
| API | `crates/api-server` order routes |
| UI | `packages/frontend` Limit/DCA tabs |

## Spec self-notes

- Permanent product ambition: Limit **and** DCA; delivery **serialized** via 3a→3d.  
- `limit_rate` encoding and aggregator-as-user auth are the two highest-risk decisions for 3a.

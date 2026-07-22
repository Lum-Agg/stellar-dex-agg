# Arb volume lift (scanner + vault size)

**Date:** 2026-07-22  
**Status:** Approved by operator delegation (“看着办”)  
**Goal:** Raise successful arb **entry notional** and opportunity capture without loosening on-chain risk floors.

## Decisions

| Topic | Choice |
|-------|--------|
| Scope this pass | Scanner size search + vault-aware size cap + operator defaults/docs |
| Quote↔sim 20bps gap | **Deferred** — needs quote-api on-chain hop validate flag (separate PR) |
| `ARB_MIN_PROFIT` | Unchanged (still post-fee gate) |
| `min_amount_out` on-chain | Still break-even (`amount_in + 1`) |

## Changes

### 1. Always size-optimize when enabled

**Before:** `optimize_round_trip` only if probe profit ≥ `ARB_MIN_PROFIT`.  
**After:** If `ARB_OPTIMIZE_AMOUNT`, always run discrete size search over `[min_amount_in, max_in]`; keep best absolute profit; still discard if final profit `< min_profit`.

Probe quote remains the fallback when optimize returns `None`.

### 2. Cap `max_amount_in` by vault base balance

When `ARB_VAULT_CONTRACT` is set, `resolve_max_amount_in` = `min(ARB_MAX_AMOUNT_IN, vault_balance(base))` with a short TTL cache (~30s) so each pair does not simulate SAC balance.

When no vault (direct aggregator mode), behavior unchanged (config ceiling only).

### 3. Operator defaults / docs

- Document recommended `ARB_MAX_AMOUNT_IN` up to vault float (e.g. 1000–1800 XLM when funded).
- Default code ceiling stays conservative unless we bump carefully — prefer env example comments over silent default jumps that break underfunded vaults.
- Note deferred P0: enable `apply_on_chain_hop_validation` behind quote-api env for arb traffic.

## Out of scope

- Lowering `ARB_MIN_PROFIT`
- Default `ARB_MAX_SPLITS > 1`
- Bridge token list expansion (ops / suggest script)

## Follow-up (done 2026-07-22)

Opt-in on-chain hop validation on quote-api:
- Query `on_chain_validate=1` or env `QUOTE_ON_CHAIN_VALIDATE=1`
- Arb default `ARB_ON_CHAIN_VALIDATE=1` appends the query param

## Success

- More opportunities reach execute when small probe is flat but larger size is profitable.
- Optimized sizes do not exceed vault float.
- Funnel / Telegram still discard on sim net < min_profit.

# Round-trip arbitrage via LumAgg aggregator

Atomic two-leg arbitrage through the **deployed aggregator contract** (`round_trip_swap`).
All volume appears on the same contract address as user swaps.

## On-chain: `round_trip_swap`

```text
round_trip_swap(user, base_token, bridge_token, amount_in, leg_out, leg_back, min_amount_out)
```

- `user`: bot G-address (`require_auth`); holds XLM/USDC float — **no custodial deposit**
- `leg_out`: `Vec<SubRoute>` base → bridge (split OK). Each `amount_in` is an **absolute** base input; they must sum to `amount_in`.
- `leg_back`: `Vec<SubRoute>` bridge → base (split OK). Each `amount_in` is a **positive weight** (quoted bridge amounts work). After `leg_out` yields actual bridge total `o1`, the contract rescales weights so executed inputs sum exactly to `o1` (last sub-route gets the remainder). Callers do **not** need to know `o1` at submit time.
- `min_amount_out`: minimum base returned (principal + profit floor)

Same `SubRoute` type for both legs — no extra fields. Semantics of `amount_in` differ by leg.

One `InvokeHostFunction` per transaction (Stellar protocol limit).

## Bot env

```bash
SNAPSHOT_REDIS_URL=...
ARB_BRIDGE_TOKENS=C...ETH...,C...BTC...   # intermediate tokens you configure
ARB_BASE_TOKENS=XLM,USDC                  # optional, defaults XLM+USDC

ARB_AGGREGATOR_CONTRACT=C...              # deployed aggregator
ARB_VAULT_CONTRACT=C...                   # optional vault for execute_round_trip
ARB_MNEMONIC_PATH=... ARB_CALLER_INDICES=0,1
# or ARB_SECRET_KEY / ARB_CALLER_SECRETS_FILE

ARB_MIN_AMOUNT_IN=100000000               # 10 XLM
ARB_MAX_AMOUNT_IN=180000000000            # 1800 XLM
ARB_OPTIMIZE_AMOUNT=1                     # max(out-in) over sample_count inputs
ARB_SAMPLE_COUNT=8
ARB_MIN_PROFIT=80000                      # default post-fee floor (base units, 7dp)
ARB_MIN_PROFIT_XLM=80000                  # optional per-base floors
ARB_MIN_PROFIT_USDC=30000
ARB_XLM_USDC_PRICE_E7=1800000             # fallback USDC/XLM for fee conversion
ARB_XLM_USDC_PRICE_REFRESH_SECS=60        # live quote refresh; 0 = fallback only

ARB_BUILD_TX=1
ARB_SUBMIT_TX=1
ARB_DRY_RUN=1

cargo run -p arbitrage --bin lumagg-arbitrage-bot
```

Operator runbook (env, monitoring, checklist): [arb-operator.md](./arb-operator.md).

## Flow

1. For each `(base, bridge)` pair: `get_route(base→bridge)` + `get_route(bridge→base)` with split
2. Profit = `leg_back.total_out - amount_in`
3. Build + submit `aggregator.round_trip_swap`

Upgrade aggregator WASM after adding `round_trip_swap` before live submit.

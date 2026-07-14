# Round-trip arbitrage via LumAgg aggregator

Atomic two-leg arbitrage through the **deployed aggregator contract** (`round_trip_swap`).
All volume appears on the same contract address as user swaps.

## On-chain: `round_trip_swap`

```text
round_trip_swap(user, base_token, bridge_token, amount_in, leg_out, leg_back, min_amount_out)
```

- `user`: bot G-address (`require_auth`); holds XLM/USDC float — **no custodial deposit**
- `leg_out`: `Vec<SubRoute>` base → bridge (split OK)
- `leg_back`: `Vec<SubRoute>` bridge → base (split OK); `amount_in` must sum to leg_out output
- `min_amount_out`: minimum base returned (principal + profit floor)

One `InvokeHostFunction` per transaction (Stellar protocol limit).

## Bot env

```bash
SNAPSHOT_REDIS_URL=...
ARB_BRIDGE_TOKENS=C...ETH...,C...BTC...   # intermediate tokens you configure
ARB_BASE_TOKENS=XLM,USDC                  # optional, defaults XLM+USDC

ARB_AGGREGATOR_CONTRACT=C...              # deployed aggregator
ARB_MNEMONIC_PATH=... ARB_CALLER_INDICES=0,1
# or ARB_SECRET_KEY / ARB_CALLER_SECRETS_FILE

ARB_MIN_AMOUNT_IN=100000000               # 10 XLM
ARB_MAX_AMOUNT_IN=180000000000            # 1800 XLM
ARB_OPTIMIZE_AMOUNT=1                     # max(out-in) over sample_count inputs
ARB_SAMPLE_COUNT=8
ARB_MIN_PROFIT_BPS=15

ARB_BUILD_TX=1
ARB_SUBMIT_TX=1
ARB_DRY_RUN=1

cargo run -p arbitrage --bin arb-scanner
```

## Flow

1. For each `(base, bridge)` pair: `get_route(base→bridge)` + `get_route(bridge→base)` with split
2. Profit = `leg_back.total_out - amount_in`
3. Build + submit `aggregator.round_trip_swap`

Upgrade aggregator WASM after adding `round_trip_swap` before live submit.

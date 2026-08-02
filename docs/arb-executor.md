# Atomic arbitrage operator stack

> **Superseded naming:** The old `arb-executor` contract design is replaced by **`aggregator.round_trip_swap`** + **`contracts/vault`** (`execute_round_trip`). See [vault README](../contracts/vault/README.md) and [scf-resubmission-budget.md](scf-resubmission-budget.md).

## Architecture

| Component | Role |
|-----------|------|
| `aggregator.round_trip_swap` | base → bridge → base in one Soroban invocation |
| `vault.execute_round_trip` | vault → caller → aggregator → vault (pooled capital; callers only need XLM for fees) |
| `crates/arbitrage` | Scanner + tx build/submit against live snapshot and pool state |

**Post-submission (Jun 2026):** vault + arb bot integration was developed after the initial SCF application. Production validation is grant-funded scope.

## Operator modes

### Mode A — Caller holds float (simple)

```bash
ARB_AGGREGATOR_CONTRACT=C...
# ARB_VAULT_CONTRACT unset
ARB_CALLER_SECRETS=...
ARB_BUILD_TX=1
ARB_SUBMIT_TX=1
ARB_DRY_RUN=1   # until live
```

Bot builds `aggregator.round_trip_swap` with `user = caller`.

### Mode B — Vault pooled capital (multi-caller, gas-only)

```bash
ARB_VAULT_CONTRACT=C...
ARB_AGGREGATOR_CONTRACT=C...
ARB_CALLER_SECRETS=...   # each only needs native XLM for fees
```

Bot builds `vault.execute_round_trip`; vault internally calls aggregator.

## Vault deploy checklist

1. Deploy vault WASM, `initialize(admin)`
2. `deposit` trading principal (e.g. XLM) into vault
3. `add_caller` for each bot public key
4. Set `ARB_VAULT_CONTRACT` + `ARB_AGGREGATOR_CONTRACT` in scanner env

## Scanner

```bash
cargo run -p arbitrage --bin lumagg-arbitrage-bot
```

Key env: `SNAPSHOT_REDIS_URL`, `ARB_BRIDGE_TOKENS`, `ARB_MIN_PROFIT`, `ARB_MAX_AMOUNT_IN`, `ARB_OPTIMIZE_AMOUNT`, `ARB_SUBMIT_DEDUP_SECS`.

## Safety

- Vault is **arb-only** — not a retail yield product
- Only trusted bot hot wallets on caller allowlist
- Start with `ARB_DRY_RUN=1`, then simulated txs, then small `ARB_MAX_AMOUNT_IN`

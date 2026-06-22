# LumAgg Arb Vault

**中文文档:** [README.zh-CN.md](README.zh-CN.md)

**This contract is for LumAgg arbitrage bots only.** It is not a general-purpose vault or yield product for end users.

For normal swaps, use the [aggregator](../aggregator/) `swap()` or `round_trip_swap()` with the user wallet holding tokens and signing. The vault solves **bot operations**: pool trading capital in one contract so multiple bot accounts only need native XLM for fees instead of each holding a large float (e.g. 1800 XLM).

## When to use what

| Scenario | Contract |
|----------|----------|
| Frontend / wallet swap | `aggregator.swap` |
| LumAgg arb bot round-trip | `vault.execute_round_trip` → calls `aggregator.round_trip_swap` internally |
| One-off manual round-trip | `aggregator.round_trip_swap` (caller holds principal) |

## Execution flow

`execute_round_trip` completes atomically in a **single contract invocation**:

```text
vault ──amount_in──► caller ──round_trip_swap──► aggregator ──► DEX
                      ▲                              │
                      └──── base_total (principal + profit) ──┘
                      caller ──base_total──► vault
```

- No standalone public `withdraw` — callers cannot drain the vault in a separate transaction.
- Profit is returned to the vault with `base_total`.
- `min_amount_out` matches the aggregator semantics (on-chain slippage / profit floor).

## Contract API

| Function | Auth | Description |
|----------|------|-------------|
| `initialize(admin)` | once at deploy | Set admin |
| `add_caller` / `remove_caller` | admin | Manage bot allowlist |
| `is_caller` | read-only | Check allowlist |
| `deposit(from, token, amount)` | `from` | Fund the vault (admin / ops wallet) |
| `execute_round_trip(...)` | allowlisted caller | **Only** arb entrypoint |
| `admin_withdraw(token, to, amount)` | admin | Emergency withdrawal |
| `upgrade` | admin | WASM upgrade |

`execute_round_trip` takes the same route args as `aggregator.round_trip_swap`, plus the `aggregator` contract address.

## Arb bot configuration

When `ARB_VAULT_CONTRACT` is set, `crates/arbitrage` builds a single-op `vault.execute_round_trip` transaction instead of calling the aggregator directly:

```bash
ARB_VAULT_CONTRACT=C...        # this vault
ARB_AGGREGATOR_CONTRACT=C...   # LumAgg aggregator
ARB_CALLER_SECRETS=...         # bot accounts; each only needs a small XLM balance for fees
ARB_BUILD_TX=1
```

If `ARB_VAULT_CONTRACT` is unset, the bot falls back to `aggregator.round_trip_swap` (callers must hold trade float themselves).

## Deploy & operate

1. Deploy vault WASM and call `initialize(admin)`.
2. `deposit` trading principal (e.g. XLM) into the vault.
3. `add_caller` for each bot public key.
4. Run the arb bot — callers **do not** need large trade-token balances, only native XLM for Soroban fees.

**Parallel callers:** concurrent submissions require vault balance ≥ `concurrent_txs × amount_in`. Example: three callers each using 500 XLM need ~1500 XLM available in the vault.

## Build & test

```bash
cargo build -p vault-contract --target wasm32v1-none --release
cargo test -p vault-contract
```

## Security

- Only add **trusted bot hot wallets** to the caller allowlist.
- Protect admin keys used for `add_caller` and `admin_withdraw`.
- Do **not** market this vault to retail users; it has no public savings / yield UX.

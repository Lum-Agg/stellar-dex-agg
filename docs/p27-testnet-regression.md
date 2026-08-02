# Protocol 27 — testnet regression checklist

Run after Stellar **Protocol 27** lands on **testnet** (before mainnet upgrade). Goal: confirm quote → build_tx → simulate/submit still work with updated network caps and RPC behavior.

**Do not upgrade mainnet WASM** until this checklist passes and you confirm with the operator.

## Prerequisites

| Item | Notes |
|------|--------|
| Testnet RPC | `https://soroban-testnet.stellar.org` or your node |
| Testnet aggregator | Deploy from `contracts/aggregator/` (separate ID from mainnet) |
| Local stack | `cargo build -p api-server -p market-data-worker --release` |
| Env | Point worker + API at testnet Redis snapshot |

## Automated smoke

```bash
# Quote + build_tx against a running local/testnet API
API=http://127.0.0.1:3100 USER_G=GYourTestnetAccount ./scripts/integrator-smoke.sh

# P27-focused script (health, quote, stats if indexer mounted)
./scripts/p27-testnet-smoke.sh
```

## Manual checklist

| # | Test | Pass | Notes |
|---|------|------|-------|
| 1 | `GET /api/v1/health` → `ok` | ☐ | |
| 2 | Quote XLM→USDC (testnet tokens) | ☐ | Record `expected_output`, `is_split` |
| 3 | `prefer_soroban=1` quote | ☐ | No Classic paths |
| 4 | `POST /build_tx` returns XDR | ☐ | Real testnet `USER_G` with sequence |
| 5 | Soroban simulate assembled XDR | ☐ | No new auth / resource errors |
| 6 | Optional: submit small swap on testnet | ☐ | Tx hash: __________ |
| 7 | Split route (if available) | ☐ | `is_split=true` still builds |
| 8 | Indexer ingests testnet events | ☐ | `lumagg-analytics-indexer --config indexer-testnet.toml run` cursor advances |
| 9 | `GET /api/v1/stats` returns rollup | ☐ | After ≥1 indexed tx |
| 10 | Arb vault `execute_round_trip` simulate | ☐ | Testnet vault + caller only |

## Regression notes template

Fill after testnet P27 cutover:

```markdown
## P27 testnet regression — YYYY-MM-DD

- Network version / ledger: ___
- RPC URL: ___
- Aggregator contract (testnet): ___
- Quote latency (p50): ___ ms
- build_tx execution mode: soroban / classic
- Issues found: none / list
- Mainnet upgrade blocked until: ___
```

## Sign-off

- [ ] All critical paths pass on testnet
- [ ] Notes committed to this file (section above)
- [ ] Mainnet deploy plan updated in [maintenance-plan.md](./maintenance-plan.md)

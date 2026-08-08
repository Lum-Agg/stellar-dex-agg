# LumAgg maintenance plan (6 months post–Tranche 3)

Grant close-out deliverable — Oct 2026 baseline.

## Scope

| Component | Owner action | Cadence |
|-----------|--------------|---------|
| Soroban RPC | Monitor latency / ledger lag; failover to backup RPC | Weekly |
| Redis pool state | `lumagg-worker` healthy; disk for AOF/RDB | Daily alert |
| API (`lumagg-api@`) | 4 instances; rate-limit logs; partner keys rotation | Monthly |
| Aggregator WASM | Upgrade only when bugfix/audit; **~20 XLM upload cost** — plan with team | As needed |
| Vault WASM | Same as aggregator; arb-only | As needed |
| `lumagg-arb` | Vault float, caller fee XLM, `ARB_MIN_PROFIT` tuning | Weekly review |
| `lumagg-indexer` | SQLite backup; cursor lag &lt; 100 ledgers | Weekly |
| Frontend `lumagg.xyz` | Redeploy on API contract / breaking changes | Per release |

## Monitoring

- **Telegram** (`deploy/telegram.env`): worker/API alerts + hourly arb P&amp;L
- **Logs**: `journalctl -u lumagg-{arb,worker,indexer,api@3100}`
- **Stats**: `/api/v1/stats` and `lumagg-analytics-indexer --config aggregator.toml status`
- **Validation report**: `./scripts/operational-validation-report.sh --output docs/operational-validation-report.md`

## Protocol 27+

- Run testnet regression after network upgrade (see `deploy/upgrade_stellar_p27.sh`)
- Re-simulate `build_tx` + one small mainnet swap before re-enabling arb submit

## Backups

```bash
# Indexer DB (daily cron example)
cp /opt/stellar-dex-aggregator/data/analytics-indexer.db \
   /opt/backups/analytics-indexer-$(date +%F).db
```

## Incident playbook

1. Quote failures → check Redis snapshot age + worker logs
2. Arb simulate failures → pool hydration / `amount_in` sum mismatch
3. Indexer stall → RPC `getEvents` errors; restart `lumagg-indexer`
4. API 429 → partner key or IP limit; scale instances if sustained

## Open-source

- Security issues: GitHub private disclosure → patch → WASM upgrade window
- Dependency updates: quarterly `cargo audit` / npm audit on SDK

## Budget notes

- Mainnet WASM uploads: budget **~20 XLM per contract upload** (aggregator size)
- RPC: self-hosted node preferred for arb latency

# LumAgg SCF Grant — Final Report (draft)

**Project:** Stellar DEX Aggregator (LumAgg)  
**Grant total:** $90,000 · Tranches Jul–Oct 2026  
**Status:** Draft — fill evidence links before submission

---

## Executive summary

LumAgg delivers a production Stellar DEX aggregator with split routing across Soroban AMMs, a public REST API and TypeScript SDK, swap UI, on-chain analytics indexer, and a mainnet atomic arbitrage vault + operator stack.

---

## Deliverable index

| # | Deliverable | Evidence | Status |
|---|-------------|----------|--------|
| D1 | Benchmark matrix | [scf-benchmark-results.md](./scf-benchmark-results.md) | ✅ |
| D2 | Integrator guide + OpenAPI | [integrator-guide.md](./integrator-guide.md), [openapi.yaml](./openapi.yaml), [external smoke evidence](./evidence/d2-integrator-smoke/) | ✅ code · ✅ external tester |
| D3 | Swap UI | https://lumagg.xyz | ✅ |
| D4 | Analytics indexer v0 | [analytics-indexer.md](./analytics-indexer.md), [sample-indexer-export.json](./sample-indexer-export.json) | ✅ |
| D5 | npm SDK | [packages/sdk](../packages/sdk), npm: [`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk) `0.2.0` | ✅ |
| D6 | Vault + arb mainnet | [arb-operator.md](./arb-operator.md), [arb-evidence-snapshot.md](./arb-evidence-snapshot.md) | ✅ |
| D7 | ≥2 integrator pilots | [integrator-pilots.md](./integrator-pilots.md) | ✅ |
| D8 | Public stats API | https://lumagg.xyz/stats, `/api/v1/stats` | ✅ · ☐ 30d data |
| D9 | Third-party audit | [audit-scope.md](./audit-scope.md), report PDF | ☐ |
| D10 | Close-out kit | [maintenance-plan.md](./maintenance-plan.md), [docker-compose.selfhost.yml](../docker-compose.selfhost.yml), demo video | partial |

---

## Production endpoints

| Service | URL |
|---------|-----|
| API | https://api.lumagg.xyz |
| Web | https://lumagg.xyz |
| Stats | https://api.lumagg.xyz/api/v1/stats |

## Mainnet contracts

| Contract | ID |
|----------|-----|
| Aggregator | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` |
| Arb vault | `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN` |

---

## Demo video

- **URL:** _(add after recording — see [demo-video-script.md](./demo-video-script.md))_
- **Duration:** ~5 minutes

---

## Audit

- **Budget:** $16,000
- **Scope:** [audit-scope.md](./audit-scope.md)
- **Report:** _(link PDF when complete)_

---

## Protocol 27

- Testnet regression: [p27-testnet-regression.md](./p27-testnet-regression.md)
- Notes: _(fill after testnet upgrade)_

---

## Maintenance (6 months)

See [maintenance-plan.md](./maintenance-plan.md): RPC, monitoring, WASM upgrade path (operator confirmation required).

---

## Appendix

- Full checklist: [grant-closeout-checklist.md](./grant-closeout-checklist.md)
- Budget source: [scf-resubmission-budget.md](./scf-resubmission-budget.md)

# Grant close-out checklist (SCF Tranche 1–3)

Living checklist — tick as evidence lands in repo or production.

## Tranche 1 · Jul 31, 2026

- [x] D1 Benchmark matrix + refresh ([scf-benchmark-results.md](./scf-benchmark-results.md))
- [x] D2 OpenAPI + [integrator-guide.md](./integrator-guide.md) + `prefer_soroban`
- [ ] D2 ≥1 external integrator: `USER_G=G... ./scripts/integrator-smoke.sh` + feedback
- [x] D3 Swap UI (logos, balance %, explorer) — self-hosted logos via `https://api.lumagg.xyz/logos/` — **51 official** (SEP-42) + fallback letter avatars (`logo_kind`)
- [x] D4 Indexer v0 + [sample-indexer-export.json](./sample-indexer-export.json)

## Tranche 2 · Aug 31, 2026

- [x] D5 SDK code + examples (`packages/sdk`)
- [ ] D5 `npm publish @lumagg/sdk`
- [x] D6 Vault + arb mainnet ([arb-operator.md](./arb-operator.md))
- [x] D6 Evidence: [arb-evidence-snapshot.md](./arb-evidence-snapshot.md) (26 SUCCESS since Jul 13)
- [ ] D7 ≥2 integrator pilots ([integrator-pilots.md](./integrator-pilots.md))

## Tranche 3 · Oct 15, 2026

- [x] D8 `/api/v1/stats` + https://lumagg.xyz/stats (after deploy)
- [ ] D8 ≥30 days indexed data
- [ ] D9 Audit report ([audit-scope.md](./audit-scope.md)) — **budget $16k**
- [ ] D10 Demo video (~5 min) — [demo-video-script.md](./demo-video-script.md)
- [x] D10 [maintenance-plan.md](./maintenance-plan.md)
- [x] D10 [docker-compose.selfhost.yml](../docker-compose.selfhost.yml)
- [ ] D10 P27 testnet regression — [p27-testnet-regression.md](./p27-testnet-regression.md)
- [ ] D10 Final report — [scf-final-report.md](./scf-final-report.md)

## Deploy after code changes

```bash
./deploy_server.sh api
./deploy_site.sh
./deploy_indexer.sh   # if indexer binary changed
```

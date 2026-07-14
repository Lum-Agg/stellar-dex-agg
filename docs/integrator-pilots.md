# Integrator pilot checklist (Tranche 2 Deliverable 7)

Track **≥2** external integrations (wallet, swap UI, or trading bot) using LumAgg REST API or npm SDK.

## Pilot template

| Field | Pilot A | Pilot B |
|-------|---------|---------|
| Name / category | e.g. internal reference app | e.g. wallet partner |
| Integration surface | REST / SDK | |
| Quote + build_tx | ☐ | ☐ |
| Demo link or PR | | |
| Feedback captured | | |

## Minimum acceptance

1. Partner identified (name or anonymized role).
2. Completed **quote → build_tx** using [integrator-guide.md](./integrator-guide.md) only.
3. One line of feedback incorporated into SDK or docs.

## Reference self-integration (Pilot A)

```bash
npx tsx packages/sdk/examples/quote-build.ts
```

## Evidence to attach

- Screenshot or curl log of successful `build_tx` `unsigned_tx_xdr` prefix.
- Or folder from `OUT=./evidence/pilot-b USER_G=G... ./scripts/integrator-smoke.sh`
- GitHub issue / email quote from external developer (Pilot B).

## Message template (send to friend)

> Hi — I'm validating our Stellar swap API for a grant deliverable. Could you run this once on your machine?
>
> 1. Clone https://github.com/Lum-Agg/stellar-dex-agg (or pull latest)
> 2. `chmod +x scripts/integrator-smoke.sh`
> 3. `USER_G=你的主网G地址 OUT=./lumagg-evidence ./scripts/integrator-smoke.sh`
>
> You need a funded mainnet account (sequence on chain); no need to sign/submit the tx.
> Send me the terminal output or zip the `lumagg-evidence/` folder. Takes ~1 minute.

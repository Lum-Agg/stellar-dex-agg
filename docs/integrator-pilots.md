# Integrator integration validation (Tranche 2 Deliverable 7)

Validate the two adoption paths approved in the SCF submission. This deliverable
requires one in-repo reference path and one external validation path; it does not
require onboarding two external partners.

## Validation template

| Field | Path A: reference client | Path B: external validation |
|-------|--------------------------|-----------------------------|
| Name / category | LumAgg UI or SDK demo | Developer, reviewer, or community integrator |
| Integration surface | REST or SDK | REST or SDK |
| Quote + build_tx | ☐ | ☐ |
| Reproducible evidence | | |
| Feedback incorporated | n/a | |

## Minimum acceptance

1. Path A documents the existing UI or SDK demo completing **quote → build_tx**.
2. Path B identifies an external tester by name or anonymized role and documents
   the same flow using the published docs, public API, or self-hosted API.
3. Both paths have reproducible steps and evidence.
4. At least one Path B feedback item is incorporated into the SDK or integrator guide.
5. The self-host quickstart and an under-30-minute walkthrough remain published.

## Path A: reference integration

```bash
npx tsx packages/sdk/examples/quote-build.ts
```

The production swap UI at <https://lumagg.xyz> is also a valid in-repo reference
path because it completes the same quote and unsigned-XDR build flow.

## Path B: external validation

The external tester may use either `@lumagg/sdk` or the REST smoke script. A
public partnership, production integration, signed transaction, or SaaS
onboarding commitment is not required.

## Evidence to attach

- Screenshot or curl log of successful `build_tx` `unsigned_tx_xdr` prefix.
- Or folder from `OUT=./evidence/path-b USER_G=G... ./scripts/integrator-smoke.sh`.
- One feedback sentence from the external tester and the resulting docs or SDK change.

## Message template (send to friend)

> Hi — I'm validating our Stellar swap API for a grant deliverable. Could you run this once on your machine?
>
> 1. Clone https://github.com/Lum-Agg/stellar-dex-agg (or pull latest)
> 2. `chmod +x scripts/integrator-smoke.sh`
> 3. `USER_G=你的主网G地址 OUT=./lumagg-evidence ./scripts/integrator-smoke.sh`
>
> You need a funded mainnet account (sequence on chain); no need to sign/submit the tx.
> Send me the terminal output or zip the `lumagg-evidence/` folder. Takes ~1 minute.

# Integrator integration validation (Tranche 2 Deliverable 7)

Validate the two adoption paths approved in the SCF submission. This deliverable
requires one in-repo reference path and one external validation path; it does not
require onboarding two external partners.

## Validation template

| Field | Path A: reference client | Path B: external validation |
|-------|--------------------------|-----------------------------|
| Name / category | LumAgg SDK CLI example | External friend / non-founder tester |
| Integration surface | `@lumagg/sdk` `0.2.0` | Published REST smoke script |
| Quote + build_tx | ✅ | ✅ |
| Reproducible evidence | [`d7-reference-sdk`](./evidence/d7-reference-sdk/) | [`d2-integrator-smoke`](./evidence/d2-integrator-smoke/) |
| Feedback incorporated | The CLI now fails unless it produces the full unsigned-XDR path | Public G-address only; no secret, signing, or submission required |

## Minimum acceptance

1. Path A documents the existing UI or SDK demo completing **quote → build_tx**.
2. Path B identifies an external tester by name or anonymized role and documents
   the same flow using the published docs, public API, or self-hosted API.
3. Both paths have reproducible steps and evidence.
4. At least one Path B feedback item is incorporated into the SDK or integrator guide.
5. The self-host quickstart and an under-30-minute walkthrough remain published.

## Path A: reference integration

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
```

On 2026-07-31 this path completed against the production API using the in-repo
SDK `0.2.0`: a two-route split quote was returned and `build_tx` produced a
Soroban unsigned XDR. The captured command and output are in
[`docs/evidence/d7-reference-sdk`](./evidence/d7-reference-sdk/).

The example now requires `USER_G` and exits non-zero if the build cannot
complete. It no longer reports success after merely obtaining a quote.

## Path B: external validation

The existing external validation was performed on 2026-07-27 by a non-founder
tester using the published REST smoke script. It produced a split Soroswap plus
Aquarius CLMM quote and a valid unsigned transaction XDR. The complete,
publicly reproducible request and response files are in
[`docs/evidence/d2-integrator-smoke`](./evidence/d2-integrator-smoke/).

This evidence is reused from Tranche 1 because it is the same approved external
`quote → build_tx` integration path; it is not represented as a second tester
or a new production partnership.

The validation clarified that the acceptance path should stop at unsigned XDR.
The script and guide therefore state explicitly that only a public, funded
G-address is needed: no secret key, signing, transaction submission, or funds
are requested from the tester.

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
> Only the public G-address is used. Do not send a secret key.
> Send me the terminal output or zip the `lumagg-evidence/` folder. Takes ~1 minute.

## Completion

Both approved paths complete `quote → build_tx`, have reproducible evidence,
and use the published under-30-minute integration documentation. D7 is complete.

# SCF Build - Tranche 2 Completion Draft

Prepared for the **August 31, 2026** completion date. This draft covers only
the approved Tranche 2 deliverables and does not repeat the Tranche 1 UI,
benchmark, or analytics-indexer deliverables.

## Tranche Deliverables

### 1. NPM TypeScript SDK and integration examples

LumAgg published the typed `@lumagg/sdk` package at version `0.2.0`. The SDK
supports the production `quote` and `build_tx` flow, typed sub-route data, and
wallet-facing unsigned XDR handling. The repository includes a minimal
quote-to-build example and a browser example using an application-owned wallet
adapter. The API reference and integration guide are published in the LumAgg
GitBook.

Evidence:

- NPM package: https://www.npmjs.com/package/@lumagg/sdk
- SDK source and examples: https://github.com/Lum-Agg/stellar-dex-agg/tree/main/packages/sdk
- API reference: https://lumagg.gitbook.io/lumagg/integrate/api-reference
- Integration guide: https://lumagg.gitbook.io/lumagg/integrate/integrator-guide

### 2. Atomic arbitrage operator stack

LumAgg deployed the arb-only Vault and a production arbitrage bot on Stellar
mainnet. The bot uses multiple fee-only caller accounts, shared Redis market
state, the production quote API, Soroban simulation, controlled submission, and
Telegram/operator monitoring. The Vault holds the trading principal; caller
accounts are not given a separate withdrawal path.

The operator documentation describes self-hosted deployment, configuration,
safe rollout, caller management, quote-to-simulation monitoring, and the
limitations of the arb-only design.

Evidence:

- Operator playbook: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arb-operator.md
- Arbitrage deployment guide: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arbitrage-deployment.md
- Mainnet Vault: `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN`
- Mainnet Aggregator: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`
- Current evidence snapshot: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/arb-evidence-snapshot.md

The evidence snapshot should be regenerated immediately before submission so
that it contains the latest successful mainnet transactions and current
deployment state.

### 3. Integrator integration validation

Two reproducible adoption paths completed the same `quote -> build_tx` flow:

- Path A: the in-repository `@lumagg/sdk` reference client.
- Path B: an external non-founder tester using the public REST smoke script and
  only a public Stellar account address.

Both paths stop at an unsigned transaction XDR. No secret key, signature, or
transaction submission is required from the tester.

Evidence:

- Validation report: https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/integrator-pilots.md
- SDK evidence: https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d7-reference-sdk
- External REST evidence (Tranche 1 D2 evidence reused for Tranche 2 D7 Path B): https://github.com/Lum-Agg/stellar-dex-agg/tree/main/docs/evidence/d2-integrator-smoke
- Self-hosted quickstart: https://lumagg.gitbook.io/lumagg/deploy/self-hosted-aggregator-quickstart

## Additional Verification

- Production application: https://lumagg.xyz
- Production API health: https://api.lumagg.xyz/api/v1/health
- Public repository: https://github.com/Lum-Agg/stellar-dex-agg
- Canonical documentation: https://lumagg.gitbook.io/

## Submission Checklist

- [x] Regenerate `docs/arb-evidence-snapshot.md` from the production server.
- [x] Confirm the latest Vault and Aggregator contract IDs.
- [x] Re-run the SDK reference example and preserve the output evidence.
- [ ] Verify all GitHub, NPM, GitBook, and production URLs in a private browser.
- [ ] Record a short public or unlisted video covering SDK, arb architecture,
  and the two integration paths.
- [ ] Paste this draft into the SCF completion form and replace any remaining
  evidence placeholders.

# Tranche 3 Preparation

SCF #44 close-out work plan for the 15 October 2026 target. This document is
an execution checklist, not a new product scope.

## Acceptance targets

| Area | Evidence required | Current state |
|---|---|---|
| D8 Analytics | Public `/stats`, CSV export, volume/tx/DEX/arb attribution, at least 30 days indexed | Dashboard is live; verify the 30-day snapshot |
| D9 Audit | Signed scope or Audit Bank engagement, final report, critical/high remediation record | Scope is drafted; obtain quotes and freeze WASM versions |
| D10 Demo | Public 5-minute video covering swap, API, analytics, arb architecture and self-hosting | Script exists; recording remains |
| D10 Testnet | Protocol 28 quote/build/simulate/submit regression notes | Checklist is ready in `p28-testnet-regression.md` |
| D10 Final report | Final report with links, metrics, transactions and audit outcome | Draft exists; fill after evidence is frozen |

## Execution order

1. **Freeze evidence inputs.** Record the current mainnet aggregator and vault
   IDs, deployed WASM hashes, release commit, RPC versions, and current arb
   configuration. Do not upgrade either contract during this step.
2. **Validate analytics.** Run the operational validation report, confirm the
   indexed date range is at least 30 days, and preserve the JSON/Markdown
   snapshot used in the final submission.
3. **Start the audit track.** Send `audit-scope.md`, the repository commit, and
   contract IDs to at least two audit candidates or the Stellar Audit Bank.
   Any critical/high remediation must be reviewed and explicitly approved
   before a contract upgrade is attempted.
4. **Run Protocol 28 testnet regression.** Complete every row in
   `p28-testnet-regression.md`; keep Order Escrow testnet-only unless its scope
   is separately approved and reviewed.
5. **Record the demo.** Use `demo-video-script.md`; show real URLs, one
   reproducible API command, the stats export, arb evidence, and self-host
   deployment links.
6. **Assemble the final report.** Replace placeholders in
   `scf-final-report.md`, link the video and audit report, and attach the final
   operational snapshot.

## Evidence commands

```bash
./scripts/operational-validation-report.sh \
  --output docs/operational-validation-report.md

curl -fsS https://api.lumagg.xyz/api/v1/stats?format=csv \
  > docs/evidence/tranche3-stats.csv

git rev-parse HEAD
sha256sum target/wasm32-unknown-unknown/release/{aggregator,vault}_contract.wasm
```

The production contract IDs and WASM hashes must be recorded separately from
the local build hashes. A local rebuild is not evidence that the deployed
contract changed.

## Explicit safety boundary

This tranche preparation does not authorize any contract upgrade. If the audit
finds a critical/high issue, stop at the report and remediation plan until the
operator approves the exact WASM, network, contract ID, and upgrade transaction.

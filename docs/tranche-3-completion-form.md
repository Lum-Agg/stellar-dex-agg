# SCF Build — Tranche 3 Completion Draft

Prepared 2026-10-08 for the 2026-10-15 target. This is a submission draft;
items marked pending require external evidence before filing.

## D8 — Public analytics and operational evidence

LumAgg exposes public analytics through the production API and dashboard.
The operational snapshot generated on 2026-10-07 confirms:

| Metric | Result |
| --- | ---: |
| Indexed coverage | 2026-07-13 → 2026-10-07 |
| Days indexed | 86 |
| Aggregator invocations | 9,473 |
| Successful transactions | 4,786 |
| DEX legs | 13,727 |
| Round trips | 4,700 |
| Entry notional (initial input; reported separately from routed volume) | $77,521.40 |
| Routed DEX volume | $265,332.42 |
| Gross surplus | $227.47 |

Evidence:

- Production dashboard: https://lumagg.xyz/stats
- Public API: https://api.lumagg.xyz/api/v1/stats
- Validation report: [`operational-validation-report.md`](./operational-validation-report.md)
- JSON snapshot: [`operational-validation-report.md.json`](./operational-validation-report.md.json)
- CSV export: [`evidence/tranche3-stats.csv`](./evidence/tranche3-stats.csv)

The separate arbitrage analytics also records **1,339,579,027 stroops /
133.9579027 XLM** of confirmed transaction fees across 9,370 historical
arbitrage transactions from 2026-07-13 through 2026-10-07, with no missing fee
values after the official Horizon backfill. This is the arbitrage gas total;
ordinary wallet swap fees are not included because the aggregator analytics
indexer does not currently expose transaction fees for those invocations.

Entry notional is included in the D8 metrics above. It is the initial input
amount for each invocation, while routed DEX volume sums the actual input of
each executed DEX leg, so the two values are intentionally reported as
separate metrics rather than added together.

## D9 — Third-party smart-contract audit

- Scope: [`audit-scope.md`](./audit-scope.md) — Aggregator and Arb Vault only
- Budget: $16,000
- Status: **Pending Audit Bank confirmation/external engagement and final report**
- Mainnet contracts: aggregator `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`;
  arb vault `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN`;
  Order Escrow remains testnet-only, is outside this audit scope, and is
  deferred to a separate audit before any future mainnet Limit/DCA deployment.

No mainnet contract upgrade is authorized by this preparation document.

## D10 — Demo, Protocol 28 regression, and close-out

- Demo video: **Pending recording and upload**; use [`demo-video-script.md`](./demo-video-script.md).
- Protocol 28 testnet regression: **Pending execution**; use [`p28-testnet-regression.md`](./p28-testnet-regression.md).
- Maintenance plan: [`maintenance-plan.md`](./maintenance-plan.md)
- Self-host deployment guide: [`deployment-overview.md`](./deployment-overview.md)
- Final report: [`scf-final-report.md`](./scf-final-report.md), to be updated with the video and audit links.

## Submission checklist

- [x] D8 public analytics and CSV snapshot
- [x] D8 at least 30 days indexed (86 days; 2026-07-13 → 2026-10-07)
- [ ] D9 audit engagement and final report
- [ ] D10 five-minute demo video
- [ ] D10 Protocol 28 testnet regression notes
- [ ] D10 final report with all external links

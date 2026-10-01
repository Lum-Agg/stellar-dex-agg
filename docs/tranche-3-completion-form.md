# SCF Build — Tranche 3 Completion Draft

Prepared 2026-10-01 for the 2026-10-15 target. This is a submission draft;
items marked pending require external evidence before filing.

## D8 — Public analytics and operational evidence

LumAgg exposes public analytics through the production API and dashboard.
The operational snapshot generated on 2026-10-01 confirms:

| Metric | Result |
| --- | ---: |
| Indexed coverage | 2026-07-13 → 2026-10-01 |
| Days indexed | 80 |
| Aggregator invocations | 8,930 |
| Successful transactions | 4,493 |
| DEX legs | 12,940 |
| Round trips | 4,435 |
| Entry notional | $73,644.02 |
| Routed DEX volume | $252,781.89 |
| Gross surplus | $224.94 |

Evidence:

- Production dashboard: https://lumagg.xyz/stats
- Public API: https://api.lumagg.xyz/api/v1/stats
- Validation report: [`operational-validation-report.md`](./operational-validation-report.md)
- JSON snapshot: [`operational-validation-report.md.json`](./operational-validation-report.md.json)
- CSV export: [`evidence/tranche3-stats.csv`](./evidence/tranche3-stats.csv)

The separate arbitrage analytics also records **1,130,279,085 stroops /
113.0279085 XLM** of confirmed transaction fees across 8,569 historical
`round_trip_swap` rows, with no missing fee values after the official Horizon
backfill.

## D9 — Third-party smart-contract audit

- Scope: [`audit-scope.md`](./audit-scope.md)
- Budget: $16,000
- Status: **Pending external engagement and final report**
- Mainnet contracts: aggregator `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`;
  arb vault `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN`

No mainnet contract upgrade is authorized by this preparation document.

## D10 — Demo, Protocol 28 regression, and close-out

- Demo video: **Pending recording and upload**; use [`demo-video-script.md`](./demo-video-script.md).
- Protocol 28 testnet regression: **Pending execution**; use [`p28-testnet-regression.md`](./p28-testnet-regression.md).
- Maintenance plan: [`maintenance-plan.md`](./maintenance-plan.md)
- Self-host deployment guide: [`deployment-overview.md`](./deployment-overview.md)
- Final report: [`scf-final-report.md`](./scf-final-report.md), to be updated with the video and audit links.

## Submission checklist

- [x] D8 public analytics and CSV snapshot
- [x] D8 at least 30 days indexed
- [ ] D9 audit engagement and final report
- [ ] D10 five-minute demo video
- [ ] D10 Protocol 28 testnet regression notes
- [ ] D10 final report with all external links

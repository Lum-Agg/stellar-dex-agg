# DefiLlama listing (LumAgg)

LumAgg is listed as a **DEX Aggregator** (volume), not TVL.

## Metric

| Field | Value |
|-------|--------|
| Dashboard | [Aggregators](https://defillama.com/aggregators) · [Stellar](https://defillama.com/dex-aggregators/chain/stellar) |
| Dimension | `dailyVolume` |
| Definition | **Entry notional** (`token_in` × `amount_in`), **not** hop-weighted routed volume |
| API | `GET https://api.lumagg.xyz/api/v1/stats?day=YYYY-MM-DD` |
| Aggregator | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` |
| Start | `2026-07-13` |

## Adapter

Ready-to-PR file: [`aggregators/lumagg/index.ts`](./aggregators/lumagg/index.ts)

Local test against upstream tree (already verified):

```text
Daily volume: ~1.02k USD (sample day)
```

## PR

Opened: https://github.com/DefiLlama/dimension-adapters/pull/8330

After merge, allow up to ~24h for the dashboard.

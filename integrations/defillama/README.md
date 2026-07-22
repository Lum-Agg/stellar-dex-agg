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

## Open the PR

```bash
git clone https://github.com/DefiLlama/dimension-adapters.git
cd dimension-adapters
# fork on GitHub, then:
git remote add mine git@github.com:<you>/dimension-adapters.git
mkdir -p aggregators/lumagg
cp /path/to/stellar-dex-aggregator/integrations/defillama/aggregators/lumagg/index.ts aggregators/lumagg/
npm i
npm test aggregators lumagg
git checkout -b lumagg-aggregator-volume
git add aggregators/lumagg
git commit -m "feat: add LumAgg Stellar DEX aggregator volume"
git push -u mine HEAD
gh pr create --repo DefiLlama/dimension-adapters --title "Add LumAgg (Stellar DEX aggregator)" --body "$(cat <<'EOF'
## Summary
- Add LumAgg aggregator volume adapter for Stellar
- Volume = swap **entry notional** (token_in amount), not hop-weighted routed volume
- Data from https://api.lumagg.xyz/api/v1/stats (on-chain indexer)

## Links
- Website: https://lumagg.xyz
- Stats: https://lumagg.xyz/stats
- Aggregator contract: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`

## Test
\`npm test aggregators lumagg\`

EOF
)"
```

After merge, allow up to ~24h for the dashboard.

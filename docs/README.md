# LumAgg Documentation

LumAgg is a liquidity aggregator for Stellar. It quotes routes across Soroban
DEXes (multi-hop and split when useful) and builds unsigned transactions for
atomic execution through the LumAgg Aggregator contract.

**Production API:** `https://api.lumagg.xyz`

**Complete docs:** `https://lumagg.gitbook.io/`

## What do you want to do?

### Integrate a wallet, dApp, or bot

1. [Integrator Guide](integrator-guide.md) — `GET /quote` → `POST /build_tx` → sign → submit
2. [JavaScript / TypeScript Example](javascript-integration-example.md) — browser integration with an application-owned wallet adapter
3. [API Reference](api-reference.md) and [OpenAPI](openapi.yaml)
4. npm SDK: [`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk)

### Self-host a quote stack

Start with [Deployment Overview](deployment-overview.md) if you are choosing
between the release binaries, or use the
[Self-hosted Aggregator Quickstart](self-hosted-aggregator-quickstart.md) to
run the split worker/API topology directly.

| Need | Guide |
| --- | --- |
| Single-process quote API | [LumAgg Swap API](lumagg-swap-api.md) |
| Split worker/API quote stack | [Self-hosted Aggregator Quickstart](self-hosted-aggregator-quickstart.md) |
| Shared market state + API replicas | [Production Aggregator](aggregator-deployment.md) |
| Public stats, swap history, and arbitrage history | [Analytics Indexer](analytics-indexer.md) |
| Aggregator / vault contracts | [Smart contracts](contracts-deployment.md) |

### Run atomic round-trip arbitrage

Start with [Arbitrage Deployment](arbitrage-deployment.md) and
[Round-trip Arbitrage](round-trip-arb.md).

## Supported liquidity

Soroswap, Aquarius (including CLMM), Phoenix, Sushi V3, and Comet. Classic
Stellar DEX routing is available for comparison and Classic-only execution; it
is not combined with Soroban legs in one transaction.

Source and issues: [LumAgg monorepo](https://github.com/Lum-Agg/stellar-dex-agg).

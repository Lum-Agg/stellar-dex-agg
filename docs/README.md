# LumAgg

LumAgg is a liquidity aggregator for Stellar. It quotes routes across Soroban
DEXes (multi-hop and split when useful) and builds unsigned transactions for
atomic execution through the LumAgg Aggregator contract.

**Production API:** `https://api.lumagg.xyz`

## Integrators start here

1. [Integrator Guide](integrator-guide.md) — `GET /quote` → `POST /build_tx` → sign → submit
2. [API Reference](api-reference.md) and [OpenAPI](openapi.yaml)
3. npm SDK: [`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk)

## Self-host / operators

| Product | When to use | Guide |
| --- | --- | --- |
| LumAgg Swap API | Local or private quote service in one process | [LumAgg Swap API](lumagg-swap-api.md) |
| Production Aggregator | Shared market state, API replicas, isolation | [Production Aggregator](aggregator-deployment.md) |
| LumAgg Arbitrage | Operator-run atomic round-trip arb | [Arbitrage Deployment](arbitrage-deployment.md) |
| Smart contracts | Aggregator + optional vault / escrow | [Contracts](contracts-deployment.md) |

## Supported liquidity

Soroswap, Aquarius (including CLMM), Phoenix, Sushi V3, and Comet. Classic
Stellar DEX routing is available for comparison and Classic-only execution; it
is not combined with Soroban legs in one transaction.

Source and issues: [LumAgg monorepo](https://github.com/Lum-Agg/stellar-dex-agg).

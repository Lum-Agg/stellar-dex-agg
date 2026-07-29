# LumAgg Documentation

LumAgg is a liquidity aggregator for Stellar. It discovers and quotes routes
across Soroban DEXes, supports multi-hop and split routing, and can build an
unsigned transaction for atomic execution through the LumAgg Aggregator
contract.

## Products

| Product | Intended use | Deployment |
| --- | --- | --- |
| LumAgg Swap API | Local development, integration testing, and a private quote service | One native binary with API and market data in one process |
| Production Aggregator | Public or high-throughput quote infrastructure | Separate market-data worker, Redis, and horizontally scalable API servers |
| LumAgg Arbitrage | Operator-run atomic round-trip arbitrage | Separate native scanner connected to a quote service and Soroban RPC |

Docker images are useful for evaluation, but native binaries and systemd are
the recommended production deployment model.

## Start Here

- Use [LumAgg Swap API](lumagg-swap-api.md) for the shortest path to a
  self-hosted quote API.
- Use [Production Aggregator Deployment](aggregator-deployment.md) when you
  need shared market state, API replicas, or failure isolation.
- Use [Arbitrage Deployment](arbitrage-deployment.md) to operate the scanner
  independently from the Aggregator services.
- Read the [Integrator Guide](integrator-guide.md) and
  [OpenAPI specification](openapi.yaml) to integrate `/quote` and `/build_tx`.

## Supported Liquidity

LumAgg currently routes across Soroswap, Aquarius pools including CLMM,
Phoenix, Sushi V3, and Comet. Classic Stellar DEX routing is available as an
optional comparison, not as a Soroban execution leg.

The source code and issue tracker are available in the
[LumAgg monorepo](https://github.com/Lum-Agg/stellar-dex-agg).

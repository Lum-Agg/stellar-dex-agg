# @stellar-dex-aggregator/sdk

TypeScript client for the [LumAgg](https://lumagg.xyz) REST API — quote, build unsigned XDR, tokens.

## Install

```bash
npm install @stellar-dex-aggregator/sdk
# or link locally during development:
cd packages/sdk && npm run build
```

## Quick start (&lt; 30 min)

```typescript
import { LumAggClient } from '@stellar-dex-aggregator/sdk';

const client = new LumAggClient({ apiUrl: 'https://api.lumagg.xyz' });

const quote = await client.quote({
  tokenIn: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', // XLM
  tokenOut: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', // USDC
  amountIn: '1000000000', // 100 XLM stroops
  slippage: 0.5,
});

const { unsignedTxXdr } = await client.buildTx({
  userPublicKey: 'G...',
  tokenIn: quote.tokenIn,
  tokenOut: quote.tokenOut,
  amountIn: quote.amountIn,
  minAmountOut: quote.minimumOutput,
  subRoutes: quote.subRoutes,
});

// Sign unsignedTxXdr with Freighter / wallet, submit to Soroban RPC or Horizon.
```

## API

| Method | REST | Description |
|--------|------|-------------|
| `isHealthy()` | `GET /health` | Liveness |
| `listTokens()` | `GET /tokens` | Routable tokens + logos |
| `quote()` | `GET /quote` | Best route; optional `preferSoroban` |
| `buildTx()` | `POST /build_tx` | Unsigned envelope XDR |
| `getStats()` | `GET /stats` | On-chain indexer rollup; optional CSV |
| `quoteAndBuild()` | quote + build_tx | One-call integrator flow |

Partner rate limit: pass `apiKey` in constructor → `X-API-Key` header (60 req/s).

## Examples

```bash
npx tsx packages/sdk/examples/basic-usage.ts
npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/stats.ts
```

## Docs

- [Integrator guide](../../docs/integrator-guide.md)
- [OpenAPI](../../docs/openapi.yaml)

## License

Apache-2.0

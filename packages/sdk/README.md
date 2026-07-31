# @lumagg/sdk

TypeScript client for the [LumAgg](https://lumagg.xyz) REST API — quote, build unsigned XDR, wallet helpers, submit + poll.

## Install

```bash
npm install @lumagg/sdk
# or link locally during development:
cd packages/sdk && npm run build
```

## Quick start (&lt; 30 min)

```typescript
import { LumAggClient } from '@lumagg/sdk';

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

// Sign with Freighter / wallet, then:
// const { hash } = await client.submitTx({ signedTxXdr });
// await client.waitForTx(hash);
```

## API

| Method | REST | Description |
|--------|------|-------------|
| `isHealthy()` | `GET /health` | Liveness |
| `listTokens()` | `GET /tokens` | Routable tokens + logos |
| `quote()` | `GET /quote` | Best route; optional `preferSoroban` |
| `buildTx()` | `POST /build_tx` | Unsigned envelope XDR |
| `quoteAndBuild()` | quote + build_tx | One-call integrator flow |
| `getBalance()` | `GET /balance` | SAC balance + `hasTrustline` |
| `getBalances()` | `GET /balances` | Common-token batch |
| `getAccount()` | `GET /account` | Sequence number |
| `getClassicAsset()` | `GET /classic_asset` | SAC → code/issuer |
| `getLatestLedger()` | `GET /ledger/latest` | Latest ledger sequence |
| `listOrders()` | `GET /orders` | Indexed Limit orders for a wallet |
| `buildCreateOrder()` | `POST /orders/build_create` | Unsigned Limit creation XDR |
| `buildCancelOrder()` | `POST /orders/build_cancel` | Unsigned Limit cancellation XDR |
| `listDcaOrders()` | `GET /dca` | Indexed DCA orders for a wallet |
| `buildCreateDca()` | `POST /dca/build_create` | Unsigned DCA creation XDR |
| `buildCancelDca()` | `POST /dca/build_cancel` | Unsigned DCA cancellation XDR |
| `submitTx()` | `POST /submit_tx` | Fast enqueue signed XDR |
| `getTxStatus()` | `GET /tx_status` | One-shot status |
| `waitForTx()` | polls `/tx_status` | Until SUCCESS / FAILED / timeout |
| `getStats()` | `GET /stats` | On-chain indexer rollup; optional CSV |
| `listSwaps()` / orders / prices | see OpenAPI | |

Order build methods return unsigned XDR for the configured Order Escrow
contract. For DCA, `minOutPerInE7` is optional and defaults to `"0"` (market
execution). All token amounts and rate fields are integer strings.

Partner rate limit: pass `apiKey` in constructor → `X-API-Key` header (60 req/s).

## Examples

```bash
npx tsx packages/sdk/examples/basic-usage.ts
npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/stats.ts

# Freighter browser demo (sign + optional submit):
cd packages/sdk/examples/browser-swap && npm install && npm run dev
```

## Docs

- [Integrator guide](../../docs/integrator-guide.md)
- [OpenAPI](../../docs/openapi.yaml)

## License

Apache-2.0

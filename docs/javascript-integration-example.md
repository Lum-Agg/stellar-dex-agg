# JavaScript / TypeScript Integration

This is the smallest browser-side integration flow for a third-party app:

1. The app asks LumAgg for a route.
2. LumAgg builds an unsigned Stellar transaction.
3. The app passes the XDR to its wallet adapter for signing.
4. The app submits the signed XDR and waits for confirmation.

LumAgg does not need the user's secret key. The app only sends the user's
public address when building the transaction.

## Install

```bash
npm install @lumagg/sdk
```

## Minimal browser example

The wallet is intentionally represented by a small adapter interface. The
integrator can connect it to any Stellar wallet flow it already uses.

```ts
import { LumAggClient } from '@lumagg/sdk';

const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const PUBLIC_NETWORK = 'Public Global Stellar Network ; September 2015';

type WalletAdapter = {
  address: string;
  signTransaction: (unsignedXdr: string, networkPassphrase: string) => Promise<string>;
};

const lumagg = new LumAggClient({
  apiUrl: 'https://api.lumagg.xyz',
  // apiKey: 'partner-key', // optional partner rate-limit key
});

export async function swapXlmToUsdc(wallet: WalletAdapter, amountIn: string) {
  // Amounts are integer strings in the token's smallest unit.
  const { quote, tx } = await lumagg.quoteAndBuild({
    tokenIn: XLM,
    tokenOut: USDC,
    amountIn,
    slippage: 0.5,       // percentage, not basis points
    preferSoroban: true, // omit this to allow Classic SDEX routes too
    maxHops: 3,
    maxSplits: 2,
    userPublicKey: wallet.address,
  });

  console.log('Expected output:', quote.expectedOutput);
  console.log('Route legs:', quote.subRoutes.length);
  console.log('Execution:', tx.execution);

  // LumAgg returns unsigned XDR. The wallet signs it locally.
  const signedXdr = await wallet.signTransaction(
    tx.unsignedTxXdr,
    PUBLIC_NETWORK,
  );

  // Alternatively, submit signedXdr through the wallet's own Stellar RPC.
  const submitted = await lumagg.submitTx({ signedTxXdr });
  const result = await lumagg.waitForTx(submitted.hash);

  if (!result.confirmed) {
    throw new Error(result.error || `Transaction ${submitted.hash} failed`);
  }

  return { hash: submitted.hash, expectedOutput: quote.expectedOutput };
}
```

## Wallet integration boundary

The wallet adapter is application-owned. It can wrap a browser extension,
mobile wallet, WalletConnect session, or a wallet service. LumAgg only requires:

- the user's public Stellar `G...` address;
- a signed XDR returned by the wallet;
- the same Stellar network passphrase used by the API and wallet.

The wallet must not expose a secret key to the application or LumAgg. For swaps
into classic-backed assets such as USDC, the user may need an asset trustline
before signing the swap.

## Quote and build separately

If the application needs to inspect or modify the quote before building, call
the two methods separately:

```ts
const quote = await lumagg.quote({
  tokenIn: XLM,
  tokenOut: USDC,
  amountIn: '100000000',
  slippage: 0.5,
  preferSoroban: true,
});

const tx = await lumagg.buildTx({
  userPublicKey: wallet.address,
  tokenIn: quote.tokenIn,
  tokenOut: quote.tokenOut,
  amountIn: quote.amountIn,
  minAmountOut: quote.minimumOutput,
  subRoutes: quote.subRoutes,
});
```

## Notes

- `amountIn`, `expectedOutput`, and `minimumOutput` are integer strings in the token's smallest unit.
- `slippage` is expressed as a percentage, for example `0.5` means 0.5%.
- `preferSoroban: true` excludes Classic SDEX paths.
- `maxHops` and `maxSplits` are optional route-complexity controls.
- Do not cache quotes for long periods. Build and sign soon after quoting.
- Public API documentation: [OpenAPI](./openapi.yaml).
- Full integration guide: [Integrator Guide](./integrator-guide.md).

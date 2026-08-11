# JavaScript / TypeScript Integration

This is the smallest browser-side integration flow for a third-party app:

1. The app asks LumAgg for a route.
2. LumAgg builds an unsigned Stellar transaction.
3. The app passes the XDR to its wallet adapter for signing.
4. The app submits the signed XDR and waits for confirmation.

LumAgg does not need the user's secret key. The app only sends the user's
public address when building the transaction.

## Pure REST API example

No SDK is required. A browser app can call the public REST API directly with
`fetch`:

```ts
const API = 'https://api.lumagg.xyz';
const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

export async function quoteAndBuild(userPublicKey: string, amountIn: string) {
  const params = new URLSearchParams({
    token_in: XLM,
    token_out: USDC,
    amount_in: amountIn,
    slippage: '0.5',
    max_hops: '3',
    max_splits: '2',
    // prefer_soroban is omitted: the default is false.
  });

  const quoteResponse = await fetch(`${API}/api/v1/quote?${params}`);
  const quoteJson = await quoteResponse.json();
  if (!quoteJson.success) throw new Error(quoteJson.error || 'Quote failed');

  const quote = quoteJson.data;
  const buildResponse = await fetch(`${API}/api/v1/build_tx`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user_public_key: userPublicKey,
      token_in: XLM,
      token_out: USDC,
      amount_in: quote.amount_in,
      min_amount_out: quote.minimum_output,
      sub_routes: quote.sub_routes,
    }),
  });
  const buildJson = await buildResponse.json();
  if (!buildJson.success) throw new Error(buildJson.error || 'Build failed');

  return { quote, unsignedTxXdr: buildJson.data.unsigned_tx_xdr };
}
```

The returned `unsigned_tx_xdr` is passed to the app's wallet adapter for
signing. We recommend submitting the signed XDR directly to a Stellar public
Soroban RPC. LumAgg's `POST /api/v1/submit_tx` endpoint is available as a
convenience fallback.

`prefer_soroban` defaults to `false`, so the normal API returns the best route
across the supported venues. Set `prefer_soroban=1` only when the integration
specifically requires Soroban-only routing; this is primarily used by the
LumAgg arbitrage bot and should not be enabled by ordinary frontend swaps
without a reason.

## Wallet integration boundary

The wallet adapter is application-owned. It can wrap a browser extension,
mobile wallet, WalletConnect session, or a wallet service. LumAgg only requires:

- the user's public Stellar `G...` address;
- a signed XDR returned by the wallet;
- the same Stellar network passphrase used by the API and wallet.

The wallet must not expose a secret key to the application or LumAgg. For swaps
into classic-backed assets such as USDC, the user may need an asset trustline
before signing the swap.

## Signing with Stellar Wallets Kit

`@creit.tech/stellar-wallets-kit` provides one interface for multiple Stellar
wallets. It is independent from `@lumagg/sdk`; the REST example above can be
used with it directly.

```bash
npm install @creit.tech/stellar-wallets-kit
```

```ts
import { StellarWalletsKit } from '@creit.tech/stellar-wallets-kit';
import { FreighterModule } from '@creit.tech/stellar-wallets-kit/modules/freighter';
import { LobstrModule } from '@creit.tech/stellar-wallets-kit/modules/lobstr';
import { xBullModule } from '@creit.tech/stellar-wallets-kit/modules/xbull';
import { Networks } from '@creit.tech/stellar-wallets-kit/types';

StellarWalletsKit.init({
  network: Networks.PUBLIC,
  modules: [new FreighterModule(), new LobstrModule(), new xBullModule()],
});

export async function signWithWalletsKit(unsignedTxXdr: string) {
  const { address } = await StellarWalletsKit.authModal();
  if (!address) throw new Error('Wallet did not return a public address');

  const { signedTxXdr } = await StellarWalletsKit.signTransaction(unsignedTxXdr, {
    networkPassphrase: Networks.PUBLIC,
    address,
  });

  return { address, signedTxXdr };
}
```

After signing, submit directly to a Stellar public Soroban RPC:

```bash
npm install @stellar/stellar-sdk
```

```ts
import { Networks, TransactionBuilder, rpc } from '@stellar/stellar-sdk';

const transaction = TransactionBuilder.fromXDR(signedTxXdr, Networks.PUBLIC);
const server = new rpc.Server('https://mainnet.sorobanrpc.com');
let submitted = await server.sendTransaction(transaction);

// Retry temporary RPC backpressure. Do not retry ERROR responses.
for (let attempt = 0;
     submitted.status === 'TRY_AGAIN_LATER' && attempt < 30;
     attempt += 1) {
  await new Promise((resolve) => setTimeout(resolve, 1000));
  submitted = await server.sendTransaction(transaction);
}

if (submitted.status !== 'PENDING' && submitted.status !== 'DUPLICATE') {
  throw new Error(`Transaction submission failed: ${submitted.status}`);
}

console.log('Submitted:', submitted.hash);
```

The app can poll the same RPC with `getTransaction(submitted.hash)` until the
status is `SUCCESS` or `FAILED`. The exact RPC endpoint can be chosen by the
integrator; use a public Soroban RPC for the same network as the wallet.

As a fallback, the signed XDR can be sent to LumAgg's
`POST /api/v1/submit_tx` endpoint with `{ "signed_tx_xdr": "..." }`.

## Signing with Freighter directly

If the app only supports Freighter, it can use Freighter's API without
Wallets Kit:

```bash
npm install @stellar/freighter-api
```

```ts
import { requestAccess, signTransaction } from '@stellar/freighter-api';

export async function signWithFreighter(unsignedTxXdr: string) {
  const access = await requestAccess();
  if (access.error || !access.address) {
    throw new Error(String(access.error || 'Freighter did not return an address'));
  }

  const result = await signTransaction(unsignedTxXdr, {
    network: 'PUBLIC',
    address: access.address,
  });
  if (typeof result === 'string') {
    return { address: access.address, signedTxXdr: result };
  }
  if (result.error || !result.signedTxXdr) {
    throw new Error(String(result.error || 'Freighter signing failed'));
  }

  return { address: access.address, signedTxXdr: result.signedTxXdr };
}
```

Other wallets can be integrated by implementing the same two responsibilities:
return the user's public address and sign the unsigned transaction XDR. The
LumAgg quote and build API does not change.

## Quote and build separately

If the application needs to inspect or modify the quote before building, call
the two methods separately:

```ts
const quote = await lumagg.quote({
  tokenIn: XLM,
  tokenOut: USDC,
  amountIn: '100000000',
  slippage: 0.5,
  // preferSoroban is omitted; the API default is false.
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
- `preferSoroban` defaults to `false`; `true` is mainly for the arbitrage bot or an explicitly Soroban-only integration.
- `maxHops` and `maxSplits` are optional route-complexity controls.
- Do not cache quotes for long periods. Build and sign soon after quoting.
- Public API documentation: [OpenAPI](./openapi.yaml).
- Full integration guide: [Integrator Guide](./integrator-guide.md).

## Optional SDK integration

The npm SDK is an optional TypeScript wrapper around the same REST endpoints.
It is not required for a frontend integration. The SDK does not replace the
wallet: the application still signs the returned unsigned XDR and can submit
it directly to a Stellar public RPC.

```bash
npm install @lumagg/sdk
```

```ts
import { LumAggClient } from '@lumagg/sdk';

const client = new LumAggClient({ apiUrl: 'https://api.lumagg.xyz' });

const { quote, tx } = await client.quoteAndBuild({
  tokenIn: XLM,
  tokenOut: USDC,
  amountIn: '100000000',
  slippage: 0.5,
  // preferSoroban is omitted; the API default is false.
  userPublicKey: wallet.address,
});

const signedXdr = await wallet.signTransaction(tx.unsignedTxXdr);

// Recommended: submit signedXdr to the Stellar public RPC using
// TransactionBuilder.fromXDR(...) and rpc.Server.sendTransaction(...).
// The SDK fallback is also available:
const submitted = await client.submitTx({ signedTxXdr });
const result = await client.waitForTx(submitted.hash);
```

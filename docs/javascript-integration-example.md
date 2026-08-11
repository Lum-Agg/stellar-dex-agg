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
signing. The app can then submit the signed XDR through its own Stellar RPC or
through `POST /api/v1/submit_tx`.

`prefer_soroban` defaults to `false`, so the normal API returns the best route
across the supported venues. Set `prefer_soroban=1` only when the integration
specifically requires Soroban-only routing; this is primarily used by the
LumAgg arbitrage bot and should not be enabled by ordinary frontend swaps
without a reason.

## Optional SDK

The npm SDK is an optional TypeScript wrapper around the same REST endpoints:

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
    // preferSoroban is omitted; the API default is false.
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

After signing, submit `signedTxXdr` with the REST endpoint:

```ts
const response = await fetch('https://api.lumagg.xyz/api/v1/submit_tx', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ signed_tx_xdr: signedTxXdr }),
});

const submitted = await response.json();
if (!submitted.success) throw new Error(submitted.error || 'Submit failed');
```

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
- `preferSoroban` defaults to `false`; `true` is mainly for the arbitrage bot or an explicitly Soroban-only integration.
- `maxHops` and `maxSplits` are optional route-complexity controls.
- Do not cache quotes for long periods. Build and sign soon after quoting.
- Public API documentation: [OpenAPI](./openapi.yaml).
- Full integration guide: [Integrator Guide](./integrator-guide.md).

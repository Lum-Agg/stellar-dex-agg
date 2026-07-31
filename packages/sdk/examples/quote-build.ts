/**
 * Minimal integrator flow: quote → build_tx (unsigned XDR).
 *
 * Run:
 *   USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
 *
 * Env:
 *   USER_G (required funded public account), API_URL, AMOUNT_STROOPS
 */

import { LumAggClient } from '../src/index';

const API_URL = process.env.API_URL || 'https://api.lumagg.xyz';
const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const USER_G = process.env.USER_G;
const AMOUNT = process.env.AMOUNT_STROOPS || '100000000'; // 10 XLM

async function main() {
  if (!USER_G) {
    throw new Error('USER_G is required; provide a funded public Stellar G-address (no secret key)');
  }

  const client = new LumAggClient({ apiUrl: API_URL });

  if (!(await client.isHealthy())) {
    console.error('API not healthy:', API_URL);
    process.exit(1);
  }

  const quote = await client.quote({
    tokenIn: XLM,
    tokenOut: USDC,
    amountIn: AMOUNT,
    slippage: 0.5,
    preferSoroban: true,
  });

  console.log('Quote OK');
  console.log('  expected_out:', quote.expectedOutput);
  console.log('  is_split:', quote.isSplit);
  console.log('  legs:', quote.subRoutes.length);

  const tx = await client.buildTx({
    userPublicKey: USER_G,
    tokenIn: XLM,
    tokenOut: USDC,
    amountIn: quote.amountIn,
    minAmountOut: quote.minimumOutput,
    subRoutes: quote.subRoutes,
  });

  console.log('build_tx OK');
  console.log('  execution:', tx.execution);
  console.log('  fee stroops:', tx.fee);
  console.log('  xdr prefix:', tx.unsignedTxXdr.slice(0, 64) + '…');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

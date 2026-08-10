/**
 * Stellar DEX Aggregator SDK — Basic Usage Example
 *
 * Demonstrates the full SDK surface:
 *   1. Client setup
 *   2. Health check
 *   3. Token list
 *   4. Quote with sub-route leg rates & percentages
 *   5. Build an unsigned swap transaction
 *
 * Run:
 *   npx tsx packages/sdk/examples/basic-usage.ts
 *
 * Options (env vars):
 *   API_URL  — aggregator deployment (default: https://api.lumagg.xyz)
 *   AMOUNT   — input amount in whole tokens (default: 100)
 *   TOKEN_IN — input token contract id  (default: XLM native)
 *   TOKEN_OUT— output token contract id (default: USDC)
 *
 * Examples:
 *   AMOUNT=50 npx tsx packages/sdk/examples/basic-usage.ts
 *   API_URL=http://localhost:3000 TOKEN_IN=<contract> TOKEN_OUT=<contract> npx tsx packages/sdk/examples/basic-usage.ts
 */

import { LumAggClient, type QuoteResult, type TokenInfo } from '../src/index';

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const API_URL = process.env.API_URL || 'https://api.lumagg.xyz';

// Well-known Stellar mainnet contract ids.
const XLM_CONTRACT = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC_CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMIHG';

const AMOUNT = process.env.AMOUNT || '100';
const TOKEN_IN = process.env.TOKEN_IN || XLM_CONTRACT;
const TOKEN_OUT = process.env.TOKEN_OUT || USDC_CONTRACT;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stroopsToUnits(stroops: string, decimals = 7): string {
  const val = Number(stroops) / 10 ** decimals;
  if (val >= 1_000) return val.toFixed(2);
  if (val >= 1) return val.toFixed(4);
  return val.toFixed(Math.min(decimals, 7));
}

function formatPercent(pct: number): string {
  if (pct > 0 && pct < 0.1) return '<0.1%';
  if (pct < 10) return `${pct.toFixed(2)}%`;
  return `${pct.toFixed(1)}%`;
}

/** Human-readable exchange rate: out-tokens per 1 in-token. */
function legRate(amountIn: string, amountOut: string, inDec = 7, outDec = 7): number | null {
  const ain = BigInt(amountIn || '0');
  const aout = BigInt(amountOut || '0');
  if (ain === BigInt(0) || aout === BigInt(0)) return null;
  const inUnits = Number(ain) / 10 ** inDec;
  const outUnits = Number(aout) / 10 ** outDec;
  return outUnits / inUnits;
}

function formatRate(rate: number): string {
  if (!Number.isFinite(rate) || rate <= 0) return '—';
  if (rate >= 100) return rate.toFixed(0);
  if (rate >= 10) return rate.toFixed(2);
  if (rate >= 1) return rate.toFixed(4);
  return rate.toFixed(6);
}

function printDivider(title = ''): void {
  const w = 60;
  if (title) {
    const pad = Math.max(0, w - title.length - 2);
    console.log(`\n${'─'.repeat(4)} ${title} ${'─'.repeat(pad)}`);
  } else {
    console.log('─'.repeat(w));
  }
}

/** Build a lookup of contract id → { symbol, decimals } from the token list. */
function buildTokenMap(
  tokens: TokenInfo[],
): Map<string, { symbol: string; decimals: number }> {
  const map = new Map<string, { symbol: string; decimals: number }>();
  for (const t of tokens) {
    map.set(t.id, { symbol: t.symbol, decimals: (t as any).decimals ?? 7 });
  }
  return map;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const sdk = new LumAggClient({ apiUrl: API_URL });

  // ---- 1. Health check --------------------------------------------------
  printDivider('1. Health check');
  const healthy = await sdk.isHealthy();
  console.log(`API ${API_URL} → ${healthy ? '✅ healthy' : '❌ unreachable'}`);
  if (!healthy) {
    console.error('Aborting: API is not reachable.');
    process.exit(1);
  }

  // ---- 2. Token list (needed for decimals & symbols) --------------------
  printDivider('2. Supported tokens');
  let tokens: TokenInfo[] = [];
  const tokenMap = new Map<string, { symbol: string; decimals: number }>();
  try {
    tokens = await sdk.getTokens();
    console.log(`Fetched ${tokens.length} tokens`);
    if (tokens.length > 0) {
      const preview = tokens.slice(0, 8);
      for (const t of preview) {
        console.log(`  • ${t.symbol.padEnd(8)} ${t.name}`);
      }
      if (tokens.length > preview.length) console.log(`  … and ${tokens.length - preview.length} more`);
    }
    // Build lookup from the live token list
    for (const t of tokens) {
      tokenMap.set(t.id, { symbol: t.symbol, decimals: (t as any).decimals ?? 7 });
    }
  } catch (err: any) {
    console.warn(`⚠️  Could not fetch token list: ${err.message}`);
  }

  // Resolve token symbols & decimals dynamically
  const inToken = tokenMap.get(TOKEN_IN);
  const outToken = tokenMap.get(TOKEN_OUT);
  const inSymbol = inToken?.symbol ?? 'XLM';
  const outSymbol = outToken?.symbol ?? 'USDC';
  const inDecimals = inToken?.decimals ?? 7;
  const outDecimals = outToken?.decimals ?? 7;

  // ---- 3. Quote ---------------------------------------------------------
  printDivider(`3. Quote: ${AMOUNT} ${inSymbol} → ${outSymbol}`);

  const amountStroops = String(BigInt(AMOUNT) * BigInt(10) ** BigInt(inDecimals));

  let quote: QuoteResult;
  try {
    quote = await sdk.quote({
      tokenIn: TOKEN_IN,
      tokenOut: TOKEN_OUT,
      amountIn: amountStroops,
      slippage: 0.5,
    });
  } catch (err: any) {
    console.error(`❌ Quote failed: ${err.message}`);
    process.exit(1);
  }

  console.log(`Input:        ${AMOUNT} ${inSymbol}`);
  console.log(`Expected out: ${stroopsToUnits(quote.expectedOutput, outDecimals)} ${outSymbol}`);
  console.log(`Minimum out:  ${stroopsToUnits(quote.minimumOutput, outDecimals)} ${outSymbol}`);
  console.log(`Price impact: ${quote.priceImpact > 0 ? `~${quote.priceImpact.toFixed(2)}%` : '<0.01%'}`);
  console.log(`Split route:  ${quote.isSplit ? 'yes' : 'no'}`);
  console.log(`Compute time: ${quote.computeTimeMs}ms`);

  // ---- 3a. Sub-routes: per-leg rate & % --------------------------------
  const subRoutes = quote.subRoutes;
  if (subRoutes.length === 0) {
    console.warn('⚠️  No sub-routes returned.');
  } else {
    printDivider(`3a. Sub-routes (${subRoutes.length} leg${subRoutes.length === 1 ? '' : 's'})`);

    const totalIn = subRoutes.reduce((s, r) => s + BigInt(r.amountIn || '0'), BigInt(0));

    for (let i = 0; i < subRoutes.length; i++) {
      const leg = subRoutes[i];

      const legPct =
        totalIn > BigInt(0) ? (Number(BigInt(leg.amountIn || '0')) / Number(totalIn)) * 100 : 0;

      const rate = legRate(leg.amountIn, leg.amountOut, inDecimals, outDecimals);

      const dexLabels = leg.source;
      const pathTokens = leg.path.join(' → ');

      console.log(`\n  Leg ${i + 1}  ——  ${formatPercent(legPct)} of input`);
      console.log(`    DEX:        ${dexLabels}`);
      console.log(`    Path:       ${pathTokens}`);
      console.log(`    Amount in:  ${stroopsToUnits(leg.amountIn, inDecimals)} ${inSymbol}`);
      console.log(`    Amount out: ${stroopsToUnits(leg.amountOut, outDecimals)} ${outSymbol}`);
      if (rate !== null) {
        console.log(`    Rate:       ${formatRate(rate)} ${outSymbol} per ${inSymbol}`);
      }
    }
  }

  // ---- 4. Build unsigned tx ----------------------------------------
  printDivider('4. Build transaction (unsigned)');

  const PLACEHOLDER_PUBKEY = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
  try {
    const tx = await sdk.buildTx({
      userPublicKey: PLACEHOLDER_PUBKEY,
      tokenIn: TOKEN_IN,
      tokenOut: TOKEN_OUT,
      amountIn: quote.amountIn,
      minAmountOut: quote.minimumOutput,
      subRoutes: quote.subRoutes,
    });

    console.log(`Execution:    ${tx.execution}`);
    console.log(`Fee stroops:  ${tx.fee}`);
    console.log(`\nUnsigned XDR (first 80 chars):  ${tx.unsignedTxXdr.slice(0, 80)}…`);
    console.log('👉  Sign with wallet, then submit via Soroban RPC or Horizon');
  } catch (err: any) {
    console.warn(`⚠️  buildTx failed: ${err.message}`);
  }

  printDivider('Done');
  console.log('SDK surface exercised successfully. ✅\n');
}

main().catch((err) => {
  console.error('Unhandled error:', err);
  process.exit(1);
});

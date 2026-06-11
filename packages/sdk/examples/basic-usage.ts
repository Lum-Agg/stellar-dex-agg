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
 * Or point to a different deployment:
 *   API_URL=https://some-other-api.xyz npx tsx packages/sdk/examples/basic-usage.ts
 */

import { StellarAggregator, type QuoteResult, type SubRoute, type TokenInfo } from '../src/index';

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const API_URL = process.env.API_URL || 'https://api.lumagg.xyz';

// Well-known Stellar mainnet contract ids.
const XLM_CONTRACT = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6T2ZMYIE2QDSOYLOU4';
const USDC_CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMIHG';

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

/** Human-readable exchange rate (out-tokens per 1 in-token). */
function legRate(amountIn: string, amountOut: string, inDec = 7, outDec = 7): number | null {
  const ain = BigInt(amountIn || '0');
  const aout = BigInt(amountOut || '0');
  if (ain === 0n || aout === 0n) return null;
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

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const sdk = new StellarAggregator({ apiUrl: API_URL });

  // ---- 1. Health check --------------------------------------------------
  printDivider('1. Health check');
  const healthy = await sdk.isHealthy();
  console.log(`API ${API_URL} → ${healthy ? '✅ healthy' : '❌ unreachable'}`);
  if (!healthy) {
    console.error('Aborting: API is not reachable.');
    process.exit(1);
  }

  // ---- 2. Token list ----------------------------------------------------
  printDivider('2. Supported tokens');
  let tokens: TokenInfo[] = [];
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
  } catch (err: any) {
    console.warn(`⚠️  Could not fetch token list: ${err.message}`);
  }

  // ---- 3. Quote ---------------------------------------------------------
  printDivider('3. Quote: 100 XLM → USDC');

  const amountInXlm = '100';
  const amountStroops = String(BigInt(amountInXlm) * 10n ** 7n); // XLM decimals = 7

  let quote: QuoteResult;
  try {
    quote = await sdk.getQuote({
      tokenIn: XLM_CONTRACT,
      tokenOut: USDC_CONTRACT,
      amountIn: amountStroops,
      slippage: 0.5,
    });
  } catch (err: any) {
    console.error(`❌ Quote failed: ${err.message}`);
    process.exit(1);
  }

  console.log(`Input:        ${amountInXlm} XLM`);
  console.log(`Expected out: ${stroopsToUnits(quote.expectedOutput)} USDC`);
  console.log(`Minimum out:  ${stroopsToUnits(quote.minimumOutput)} USDC`);
  console.log(`Price impact: ${quote.priceImpact > 0 ? `~${quote.priceImpact.toFixed(2)}%` : '<0.01%'}`);
  console.log(`Split route:  ${quote.isSplit ? 'yes' : 'no'}`);
  console.log(`Compute time: ${quote.computeTimeMs}ms`);

  // ---- 3a. Sub-routes: per-leg rate & % --------------------------------
  const subRoutes = quote.subRoutes;
  if (subRoutes.length === 0) {
    console.warn('⚠️  No sub-routes returned.');
  } else {
    printDivider(`3a. Sub-routes (${subRoutes.length} leg${subRoutes.length === 1 ? '' : 's'})`);

    const totalIn = subRoutes.reduce((s, r) => s + BigInt(r.amountIn || '0'), 0n);

    for (let i = 0; i < subRoutes.length; i++) {
      const leg = subRoutes[i];

      // Percentage of total input
      const legPct =
        totalIn > 0n ? (Number(BigInt(leg.amountIn || '0')) / Number(totalIn)) * 100 : 0;

      // Human-readable exchange rate for this leg (USDC per XLM)
      // Using 7 decimals for both (Stellar standard); adjust if needed.
      const rate = legRate(leg.amountIn, leg.amountOut, 7, 7);

      const dexLabels = leg.source;
      const pathTokens = leg.path.join(' → ');

      console.log(`\n  Leg ${i + 1}  ——  ${formatPercent(legPct)} of input`);
      console.log(`    DEX:        ${dexLabels}`);
      console.log(`    Path:       ${pathTokens}`);
      console.log(`    Amount in:  ${stroopsToUnits(leg.amountIn)} XLM`);
      console.log(`    Amount out: ${stroopsToUnits(leg.amountOut)} USDC`);
      if (rate !== null) {
        console.log(`    Rate:       ${formatRate(rate)} USDC per XLM`);
      }
    }
  }

  // ---- 4. Build unsigned swap tx ----------------------------------------
  printDivider('4. Build swap transaction (unsigned)');

  // This requires a valid Stellar public key; we use a placeholder.
  const PLACEHOLDER_PUBKEY = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
  try {
    const swap = await sdk.buildSwap({
      tokenIn: XLM_CONTRACT,
      tokenOut: USDC_CONTRACT,
      amountIn: amountStroops,
      slippage: 0.5,
      userPublicKey: PLACEHOLDER_PUBKEY,
    });

    console.log(`Simulation:   ${swap.simulation.success ? '✅ success' : '❌ failed'}`);
    if (swap.simulation.actualOutput) {
      console.log(`Actual out:   ${stroopsToUnits(swap.simulation.actualOutput)} USDC`);
    }
    if (swap.simulation.fee) {
      console.log(`Fee:          ${stroopsToUnits(swap.simulation.fee)} XLM`);
    }
    if (swap.simulation.error) {
      console.log(`Sim error:    ${swap.simulation.error}`);
    }

    // The unsigned XDR would then be signed by the user's wallet (e.g.
    // Freighter, Albedo) and submitted to Horizon.
    console.log(`\nUnsigned XDR (first 80 chars):  ${swap.unsignedTxXdr.slice(0, 80)}…`);
    console.log('👉  Sign with wallet, then POST to Horizon /transactions');
  } catch (err: any) {
    console.warn(`⚠️  buildSwap failed (expected with placeholder key): ${err.message}`);
  }

  printDivider('Done');
  console.log('SDK surface exercised successfully. ✅\n');
}

main().catch((err) => {
  console.error('Unhandled error:', err);
  process.exit(1);
});

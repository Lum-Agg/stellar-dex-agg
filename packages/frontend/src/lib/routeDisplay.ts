import type { SubRoute } from '@/lib/aggregator';

// ---------------------------------------------------------------------------
// Sub-route display filtering
// ---------------------------------------------------------------------------

/**
 * Minimum input for a sub-route to be shown as a separate leg, in basis points
 * of the total input amount. Matches the router's `min_split_amount_in_bps`.
 * Legs below this threshold are "dust" and are hidden from the UI.
 */
export const MIN_DISPLAY_LEG_INPUT_BPS = 10;

/**
 * Filter sub-routes: hide legs whose input is below the dust threshold.
 * If all legs would be hidden, returns the original list unchanged.
 */
export function subRoutesForDisplay(
  routes: SubRoute[],
  totalAmountIn: string | undefined,
): SubRoute[] {
  if (!routes.length) return routes;
  const total = BigInt(totalAmountIn || '0');
  if (total === BigInt(0)) return routes;

  const minIn = (total * BigInt(MIN_DISPLAY_LEG_INPUT_BPS)) / BigInt(10000);
  const visible = routes.filter((r) => BigInt(r.amount_in || '0') >= minIn);
  return visible.length > 0 ? visible : routes;
}

// ---------------------------------------------------------------------------
// Leg percentage formatting
// ---------------------------------------------------------------------------

/**
 * Format the percentage of total input that flows through this leg.
 * Uses higher precision for small values and clamps near-zero to "<0.1%".
 */
export function formatLegPercent(percentage: number): string {
  if (percentage > 0 && percentage < 0.1) return '<0.1%';
  if (percentage < 10) return `${percentage.toFixed(2)}%`;
  return `${percentage.toFixed(1)}%`;
}

// ---------------------------------------------------------------------------
// Exchange rate calculation & formatting
// ---------------------------------------------------------------------------

/**
 * Compute a human-readable exchange rate: "out-tokens per 1 in-token".
 *
 * Accounts for token decimals so the displayed rate matches what a user
 * expects — unlike a raw stroop ratio which would be off by orders of
 * magnitude when the two tokens use different decimal places.
 *
 * @param amountIn    Input stroops for this leg.
 * @param amountOut   Output stroops for this leg.
 * @param inDecimals  Decimal places of the input token (default 7).
 * @param outDecimals Decimal places of the output token (default 7).
 * @returns The exchange rate (out-units / in-units), or null if inputs are zero.
 */
export function legExchangeRate(
  amountIn: string,
  amountOut: string,
  inDecimals = 7,
  outDecimals = 7,
): number | null {
  const ain = BigInt(amountIn || '0');
  const aout = BigInt(amountOut || '0');
  if (ain === BigInt(0) || aout === BigInt(0)) return null;
  const inUnits = Number(ain) / 10 ** inDecimals;
  const outUnits = Number(aout) / 10 ** outDecimals;
  return outUnits / inUnits;
}

/**
 * Format an exchange rate for display, choosing precision based on magnitude.
 * Large rates (≥100) use 0 decimal places; tiny rates (<1) use up to 6.
 */
export function formatExchangeRate(rate: number): string {
  if (!Number.isFinite(rate) || rate <= 0) return '—';
  if (rate >= 100) return rate.toFixed(0);
  if (rate >= 10) return rate.toFixed(2);
  if (rate >= 1) return rate.toFixed(4);
  return rate.toFixed(6);
}

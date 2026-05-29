import type { SubRoute } from '@/lib/aggregator';

/** Match router `min_split_amount_in_bps` (0.1% of total input). */
export const MIN_DISPLAY_LEG_INPUT_BPS = 10;

export function subRoutesForDisplay(
  routes: SubRoute[],
  totalAmountIn: string | undefined
): SubRoute[] {
  if (!routes.length) return routes;
  const total = BigInt(totalAmountIn || '0');
  if (total === BigInt(0)) return routes;

  const minIn = (total * BigInt(MIN_DISPLAY_LEG_INPUT_BPS)) / BigInt(10000);
  const visible = routes.filter((r) => BigInt(r.amount_in || '0') >= minIn);
  return visible.length > 0 ? visible : routes;
}

export function formatLegPercent(percentage: number): string {
  if (percentage > 0 && percentage < 0.1) return '<0.1%';
  if (percentage < 10) return `${percentage.toFixed(2)}%`;
  return `${percentage.toFixed(1)}%`;
}

/** Stroops out / stroops in (same decimals on both legs). */
export function legExchangeRate(amountIn: string, amountOut: string): number | null {
  const ain = BigInt(amountIn || '0');
  const aout = BigInt(amountOut || '0');
  if (ain === BigInt(0) || aout === BigInt(0)) return null;
  return Number(aout) / Number(ain);
}

export function formatExchangeRate(rate: number): string {
  if (!Number.isFinite(rate) || rate <= 0) return '—';
  if (rate >= 100) return rate.toFixed(0);
  if (rate >= 10) return rate.toFixed(2);
  if (rate >= 1) return rate.toFixed(4);
  return rate.toFixed(6);
}

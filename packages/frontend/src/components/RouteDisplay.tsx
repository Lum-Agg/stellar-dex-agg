'use client';

import { useEffect, useState } from 'react';
import type { QuoteData, SubRoute } from '@/lib/aggregator';
import {
  formatExchangeRate,
  formatLegPercent,
  legExchangeRate,
  subRoutesForDisplay,
} from '@/lib/routeDisplay';

const DEX_STYLES: Record<string, { label: string }> = {
  soroswap: { label: 'Soroswap' },
  aquarius_clmm: { label: 'Aquarius CLMM' },
  aquarius: { label: 'Aquarius' },
  phoenix: { label: 'Phoenix' },
  sushi: { label: 'Sushi' },
  comet: { label: 'Comet' },
  classic: { label: 'Classic DEX' },
};

function dexStyle(dex: string) {
  const key = dex.toLowerCase().replace(/\s+/g, '_');
  if (DEX_STYLES[key]) return DEX_STYLES[key];
  const label = dex.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  return { label };
}

function routeDexHops(route: SubRoute): string[] {
  if (route.dex_types?.length) {
    return route.dex_types;
  }
  if (route.source.includes('→')) {
    return route.source.split('→').map((s) => s.trim());
  }
  return [route.source];
}

function PathArrow() {
  return (
    <svg
      className="w-3 h-3 text-[var(--text-muted)] shrink-0"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      aria-hidden
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M13 7l5 5m0 0l-5 5m5-5H6"
      />
    </svg>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      className={`w-3.5 h-3.5 text-[var(--text-muted)] transition-transform ${open ? 'rotate-180' : ''}`}
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      aria-hidden
    >
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
    </svg>
  );
}

function RouteAmountPath({
  path,
  amountIn,
  amountOut,
  tokenInDecimals,
  tokenOutDecimals,
  resolveTokenSymbol,
  formatAmount,
}: {
  path: string[];
  amountIn: string;
  amountOut: string;
  tokenInDecimals: number;
  tokenOutDecimals: number;
  resolveTokenSymbol: (contractId: string) => string;
  formatAmount: (stroops: string, decimals: number) => string;
}) {
  const symbols = path.map(resolveTokenSymbol);
  if (symbols.length < 2) return null;

  const mids = symbols.slice(1, -1);

  return (
    <div className="flex flex-wrap items-center gap-1.5 text-[13px] text-[var(--text-muted)]">
      <span className="font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
        {formatAmount(amountIn, tokenInDecimals)}
      </span>
      <span className="text-[var(--text-secondary)] font-medium">{symbols[0]}</span>
      {mids.map((sym, idx) => (
        <span key={`${sym}-${idx}`} className="inline-flex items-center gap-1.5">
          <PathArrow />
          <span className="text-[var(--text-muted)] font-medium">{sym}</span>
        </span>
      ))}
      <PathArrow />
      <span className="font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
        {formatAmount(amountOut, tokenOutDecimals)}
      </span>
      <span className="text-[var(--text-secondary)] font-medium">
        {symbols[symbols.length - 1]}
      </span>
    </div>
  );
}

function routeSummaryLabel(routes: SubRoute[]): string {
  if (routes.length === 0) return '—';
  if (routes.length === 1) {
    const hops = routeDexHops(routes[0]);
    if (hops.length === 1) return dexStyle(hops[0]).label;
    return `${hops.length} hops`;
  }
  return `${routes.length} paths`;
}

export function RouteDisplay({
  quote,
  tokenInSymbol,
  tokenOutSymbol,
  tokenInDecimals = 7,
  tokenOutDecimals = 7,
  resolveTokenSymbol,
}: {
  quote: QuoteData;
  tokenInSymbol?: string;
  tokenOutSymbol: string;
  tokenInDecimals?: number;
  tokenOutDecimals?: number;
  resolveTokenSymbol: (contractId: string) => string;
}) {
  const [open, setOpen] = useState(false);

  // New quote → collapse again (avoid stale expanded state).
  useEffect(() => {
    setOpen(false);
  }, [quote.amount_in, quote.expected_output, quote.sub_routes.length]);

  const formatAmount = (stroops: string, decimals: number) => {
    const val = parseInt(stroops, 10) / 10 ** decimals;
    if (val >= 1000) return val.toFixed(2);
    if (val >= 1) return val.toFixed(4);
    return val.toFixed(Math.min(decimals, 7));
  };

  const displayRoutes = subRoutesForDisplay(quote.sub_routes, quote.amount_in);
  const hiddenLegCount = quote.sub_routes.length - displayRoutes.length;
  const outSym = tokenOutSymbol;
  const inSym =
    tokenInSymbol ||
    (quote.sub_routes[0]?.path[0] ? resolveTokenSymbol(quote.sub_routes[0].path[0]) : '') ||
    '???';
  const summary = routeSummaryLabel(displayRoutes);

  return (
    <div className="rounded-2xl border border-[var(--border)] bg-[var(--surface)]/80 px-4 py-3 space-y-2.5">
      <div className="flex justify-between text-[13px] sm:text-[14px]">
        <span className="text-[var(--text-muted)]">Price impact</span>
        <span
          className={quote.price_impact > 1 ? 'text-amber-400' : 'text-[var(--text-secondary)]'}
        >
          {quote.price_impact > 0 ? `~${quote.price_impact.toFixed(2)}%` : '< 0.01%'}
        </span>
      </div>
      <div className="flex justify-between text-[13px] sm:text-[14px]">
        <span className="text-[var(--text-muted)]">Minimum received</span>
        <span className="text-[var(--text-secondary)] font-[family-name:var(--font-mono)]">
          {formatAmount(quote.minimum_output, tokenOutDecimals)} {tokenOutSymbol}
        </span>
      </div>

      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="w-full flex items-center justify-between gap-3 pt-1 text-[13px] sm:text-[14px] hover:opacity-90 transition-opacity"
      >
        <span className="text-[var(--text-muted)]">Route</span>
        <span className="inline-flex items-center gap-2 min-w-0">
          <span className="rounded-full border border-[var(--border)] bg-[var(--bg-0)]/60 px-2.5 py-0.5 text-[12px] font-medium text-[var(--text-secondary)] truncate max-w-[10rem] sm:max-w-[14rem]">
            {summary}
          </span>
          <Chevron open={open} />
        </span>
      </button>

      {open && (
        <div className="space-y-2 pt-1 border-t border-[var(--border)]">
          {quote.compute_time_ms !== undefined && (
            <div className="text-[12px] text-[var(--text-muted)]">
              Quoted in {quote.compute_time_ms}ms
              {hiddenLegCount > 0
                ? ` · ${hiddenLegCount} dust leg${hiddenLegCount === 1 ? '' : 's'} hidden`
                : ''}
            </div>
          )}

          <div className="space-y-2">
            {displayRoutes.map((route, i) => {
              const rate = legExchangeRate(
                route.amount_in,
                route.amount_out,
                tokenInDecimals,
                tokenOutDecimals,
              );
              const hops = routeDexHops(route);

              return (
                <div
                  key={i}
                  className="rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/50 p-3"
                >
                  <div className="flex items-center justify-between gap-2 mb-1.5">
                    <div className="flex flex-wrap items-center gap-1 min-w-0">
                      {hops.map((dex, j) => {
                        const { label } = dexStyle(dex);
                        return (
                          <span key={`${dex}-${j}`} className="inline-flex items-center gap-1">
                            {j > 0 && (
                              <span className="text-[var(--text-muted)] text-[12px]">→</span>
                            )}
                            <span className="text-[13px] font-medium text-[var(--text-secondary)]">
                              {label}
                            </span>
                          </span>
                        );
                      })}
                    </div>
                    <div className="text-right shrink-0">
                      <span className="text-[13px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] block">
                        {formatLegPercent(route.percentage)}
                      </span>
                      {rate != null && inSym && outSym && (
                        <span className="text-[12px] text-[var(--text-muted)] font-[family-name:var(--font-mono)]">
                          {formatExchangeRate(rate)} {outSym}/{inSym}
                        </span>
                      )}
                    </div>
                  </div>
                  <RouteAmountPath
                    path={route.path}
                    amountIn={route.amount_in}
                    amountOut={route.amount_out}
                    tokenInDecimals={tokenInDecimals}
                    tokenOutDecimals={tokenOutDecimals}
                    resolveTokenSymbol={resolveTokenSymbol}
                    formatAmount={formatAmount}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

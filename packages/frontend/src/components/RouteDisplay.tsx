'use client';

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
  const label = dex
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
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
      className="w-3 h-3 text-zinc-600 shrink-0"
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
    <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-zinc-500">
      <span className="font-mono text-zinc-400">{formatAmount(amountIn, tokenInDecimals)}</span>
      <span className="text-zinc-300 font-medium">{symbols[0]}</span>
      {mids.map((sym, idx) => (
        <span key={`${sym}-${idx}`} className="inline-flex items-center gap-1.5">
          <PathArrow />
          <span className="text-zinc-400 font-medium">{sym}</span>
        </span>
      ))}
      <PathArrow />
      <span className="font-mono text-zinc-400">{formatAmount(amountOut, tokenOutDecimals)}</span>
      <span className="text-zinc-300 font-medium">{symbols[symbols.length - 1]}</span>
    </div>
  );
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

  return (
    <div className="surface-panel p-4 space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-[12px] font-medium text-zinc-300">Execution route</span>
        {quote.compute_time_ms !== undefined && (
          <span className="text-[11px] text-zinc-600">Quote in {quote.compute_time_ms}ms</span>
        )}
      </div>

      {quote.is_split && (
        <div className="flex items-center gap-1.5 text-[11px] text-zinc-400">
          <svg className="w-3 h-3 shrink-0 text-zinc-500" fill="currentColor" viewBox="0 0 20 20">
            <path
              fillRule="evenodd"
              d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z"
              clipRule="evenodd"
            />
          </svg>
          <span>
            Split across {displayRoutes.length} path{displayRoutes.length === 1 ? '' : 's'} for better
            execution
            {hiddenLegCount > 0
              ? ` (${hiddenLegCount} dust leg${hiddenLegCount === 1 ? '' : 's'} hidden)`
              : ''}
          </span>
        </div>
      )}

      <div className="space-y-2">
        {displayRoutes.map((route, i) => {
          const rate = legExchangeRate(route.amount_in, route.amount_out, tokenInDecimals, tokenOutDecimals);
          const hops = routeDexHops(route);

          return (
            <div key={i} className="rounded-lg border border-white/[0.06] bg-zinc-900/40 p-3">
              <div className="flex items-center justify-between gap-2 mb-1.5">
                <div className="flex flex-wrap items-center gap-1 min-w-0">
                  {hops.map((dex, j) => {
                    const { label } = dexStyle(dex);
                    return (
                      <span key={`${dex}-${j}`} className="inline-flex items-center gap-1">
                        {j > 0 && <span className="text-zinc-600 text-[10px]">→</span>}
                        <span className="text-[12px] font-medium text-zinc-300">{label}</span>
                      </span>
                    );
                  })}
                </div>
                <div className="text-right shrink-0">
                  <span className="text-[12px] text-zinc-400 font-mono block">
                    {formatLegPercent(route.percentage)}
                  </span>
                  {rate != null && inSym && outSym && (
                    <span className="text-[11px] text-zinc-500 font-mono">
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

      <div className="border-t border-white/[0.06] pt-3 space-y-1.5">
        <div className="flex justify-between text-[12px]">
          <span className="text-zinc-500">Price impact</span>
          <span className={quote.price_impact > 1 ? 'text-amber-400' : 'text-zinc-300'}>
            {quote.price_impact > 0 ? `~${quote.price_impact.toFixed(2)}%` : '< 0.01%'}
          </span>
        </div>
        <div className="flex justify-between text-[12px]">
          <span className="text-zinc-500">Minimum received</span>
          <span className="text-zinc-300 font-mono">
            {formatAmount(quote.minimum_output, tokenOutDecimals)} {tokenOutSymbol}
          </span>
        </div>
      </div>
    </div>
  );
}

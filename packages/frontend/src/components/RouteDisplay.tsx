'use client';

import type { QuoteData, SubRoute } from '@/lib/aggregator';

const DEX_STYLES: Record<string, { label: string; color: string; bg: string }> = {
  soroswap: { label: 'Soroswap', color: 'text-emerald-300', bg: 'bg-emerald-500/10 border-emerald-400/25' },
  aquarius_clmm: { label: 'Aquarius CLMM', color: 'text-cyan-300', bg: 'bg-cyan-500/10 border-cyan-400/25' },
  aquarius: { label: 'Aquarius', color: 'text-cyan-300', bg: 'bg-cyan-500/10 border-cyan-400/25' },
  phoenix: { label: 'Phoenix', color: 'text-orange-300', bg: 'bg-orange-500/10 border-orange-400/25' },
  sushi: { label: 'Sushi', color: 'text-fuchsia-300', bg: 'bg-fuchsia-500/10 border-fuchsia-400/25' },
  comet: { label: 'Comet', color: 'text-indigo-300', bg: 'bg-indigo-500/10 border-indigo-400/25' },
  classic: { label: 'Classic DEX', color: 'text-violet-300', bg: 'bg-violet-500/10 border-violet-400/25' },
};

function dexStyle(dex: string) {
  const key = dex.toLowerCase().replace(/\s+/g, '_');
  if (DEX_STYLES[key]) return DEX_STYLES[key];
  const label = dex
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
  return { label, color: 'text-slate-300', bg: 'bg-slate-500/10 border-slate-300/20' };
}

/** Prefer dex_types; fall back to source string (may be "a → b"). */
function routeDexHops(route: SubRoute): string[] {
  if (route.dex_types?.length) {
    return route.dex_types;
  }
  if (route.source.includes('→')) {
    return route.source.split('→').map((s) => s.trim());
  }
  return [route.source];
}

function routeCardStyle(hops: string[]) {
  if (hops.length === 1) return dexStyle(hops[0]).bg;
  return 'bg-slate-900/80 border-white/10';
}

function PathArrow() {
  return (
    <svg
      className="w-3 h-3 text-slate-600 shrink-0"
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
    <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-slate-400">
      <span className="font-mono text-slate-300">{formatAmount(amountIn, tokenInDecimals)}</span>
      <span className="text-slate-200 font-medium">{symbols[0]}</span>
      {mids.map((sym, idx) => (
        <span key={`${sym}-${idx}`} className="inline-flex items-center gap-1.5">
          <PathArrow />
          <span className="text-slate-300 font-medium">{sym}</span>
        </span>
      ))}
      <PathArrow />
      <span className="font-mono text-slate-300">{formatAmount(amountOut, tokenOutDecimals)}</span>
      <span className="text-slate-200 font-medium">{symbols[symbols.length - 1]}</span>
    </div>
  );
}

export function RouteDisplay({
  quote,
  tokenOutSymbol,
  tokenInDecimals = 7,
  tokenOutDecimals = 7,
  resolveTokenSymbol,
}: {
  quote: QuoteData;
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

  return (
    <div className="bg-slate-900/70 rounded-2xl border border-white/10 p-4 space-y-3 backdrop-blur-xl">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-slate-300">Execution route</span>
        {quote.compute_time_ms !== undefined && (
          <span className="text-[10px] text-slate-500">Quote in {quote.compute_time_ms}ms</span>
        )}
      </div>

      {quote.is_split && (
        <div className="flex items-center gap-1.5 text-[11px] text-amber-300/90">
          <svg className="w-3 h-3 shrink-0" fill="currentColor" viewBox="0 0 20 20">
            <path
              fillRule="evenodd"
              d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z"
              clipRule="evenodd"
            />
          </svg>
          <span>Split across {quote.sub_routes.length} paths for better execution</span>
        </div>
      )}

      <div className="space-y-2.5">
        {quote.sub_routes.map((route, i) => {
          const hops = routeDexHops(route);
          const cardBg = routeCardStyle(hops);

          return (
            <div key={i} className={`rounded-lg border p-3 ${cardBg}`}>
              <div className="flex items-center justify-between gap-2 mb-1.5">
                <div className="flex flex-wrap items-center gap-1 min-w-0">
                  {hops.map((dex, j) => {
                    const { label, color } = dexStyle(dex);
                    return (
                      <span key={`${dex}-${j}`} className="inline-flex items-center gap-1">
                        {j > 0 && <span className="text-slate-500 text-[10px]">→</span>}
                        <span className={`text-xs font-medium ${color}`}>{label}</span>
                      </span>
                    );
                  })}
                </div>
                <span className="text-[11px] text-slate-400 font-mono shrink-0">
                  {route.percentage < 10
                    ? route.percentage.toFixed(1)
                    : route.percentage.toFixed(0)}
                  %
                </span>
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

      <div className="border-t border-white/10 pt-3 space-y-1.5">
        <div className="flex justify-between text-xs">
          <span className="text-slate-500">Price impact</span>
          <span className={quote.price_impact > 1 ? 'text-amber-300' : 'text-emerald-300'}>
            {quote.price_impact > 0 ? `~${quote.price_impact.toFixed(2)}%` : '< 0.01%'}
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span className="text-slate-500">Minimum received</span>
          <span className="text-slate-300 font-mono">
            {formatAmount(quote.minimum_output, tokenOutDecimals)} {tokenOutSymbol}
          </span>
        </div>
      </div>
    </div>
  );
}

'use client';

import type { QuoteData } from '@/lib/aggregator';

export function RouteDisplay({
  quote,
  tokenInSymbol,
  tokenOutSymbol,
}: {
  quote: QuoteData;
  tokenInSymbol: string;
  tokenOutSymbol: string;
}) {
  const formatAmount = (stroops: string, decimals: number = 7) => {
    const val = parseInt(stroops) / 10 ** decimals;
    if (val >= 1000) return val.toFixed(2);
    if (val >= 1) return val.toFixed(4);
    return val.toFixed(7);
  };

  // Map source names to display names and colors
  const sourceDisplay = (source: string) => {
    if (source.includes('soroswap')) return { name: 'Soroswap', color: 'text-green-400', bg: 'bg-green-400/10 border-green-400/20' };
    if (source.includes('aquarius')) return { name: 'Aquarius', color: 'text-cyan-400', bg: 'bg-cyan-400/10 border-cyan-400/20' };
    if (source.includes('phoenix')) return { name: 'Phoenix', color: 'text-orange-400', bg: 'bg-orange-400/10 border-orange-400/20' };
    if (source.includes('classic')) return { name: 'Classic DEX', color: 'text-purple-400', bg: 'bg-purple-400/10 border-purple-400/20' };
    return { name: source, color: 'text-gray-400', bg: 'bg-gray-400/10 border-gray-400/20' };
  };

  return (
    <div className="bg-[#12131a] rounded-2xl border border-white/5 p-4 space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-gray-400">Route</span>
        {quote.compute_time_ms !== undefined && (
          <span className="text-[10px] text-gray-600">{quote.compute_time_ms}ms</span>
        )}
      </div>

      {/* Split indicator */}
      {quote.is_split && (
        <div className="flex items-center gap-1.5 text-[11px] text-amber-400/80">
          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
            <path fillRule="evenodd" d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z" clipRule="evenodd" />
          </svg>
          <span>Split across {quote.sub_routes.length} paths for better execution</span>
        </div>
      )}

      {/* Routes */}
      <div className="space-y-2">
        {quote.sub_routes.map((route, i) => {
          const display = sourceDisplay(route.source);
          return (
            <div key={i} className={`rounded-lg border p-3 ${display.bg}`}>
              <div className="flex items-center justify-between mb-1.5">
                <span className={`text-xs font-medium ${display.color}`}>
                  {display.name}
                </span>
                <span className="text-[11px] text-gray-400 font-mono">
                  {route.percentage.toFixed(0)}%
                </span>
              </div>
              <div className="flex items-center gap-1.5 text-[11px] text-gray-400">
                <span className="font-mono">{formatAmount(route.amount_in)}</span>
                <span>{tokenInSymbol}</span>
                <svg className="w-3 h-3 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
                </svg>
                <span className="font-mono">{formatAmount(route.amount_out)}</span>
                <span>{tokenOutSymbol}</span>
              </div>
            </div>
          );
        })}
      </div>

      {/* Summary */}
      <div className="border-t border-white/5 pt-3 space-y-1.5">
        <div className="flex justify-between text-xs">
          <span className="text-gray-500">Price Impact</span>
          <span className={quote.price_impact > 1 ? 'text-amber-400' : 'text-green-400'}>
            {quote.price_impact > 0 ? `~${quote.price_impact.toFixed(2)}%` : '< 0.01%'}
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span className="text-gray-500">Minimum received</span>
          <span className="text-gray-300 font-mono">
            {formatAmount(quote.minimum_output)} {tokenOutSymbol}
          </span>
        </div>
      </div>
    </div>
  );
}

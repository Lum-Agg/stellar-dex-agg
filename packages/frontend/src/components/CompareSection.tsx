'use client';

/** Illustrative comparison for marketing — not a live quote. */
const EXAMPLE_IN = '10 XLM';
const SINGLE_OUT = '1.4521';
const SPLIT_OUT = '1.4753';
const IMPROVEMENT_PCT = (
  ((parseFloat(SPLIT_OUT) - parseFloat(SINGLE_OUT)) / parseFloat(SINGLE_OUT)) *
  100
).toFixed(1);

const SINGLE_ROUTE = {
  dex: 'Soroswap',
  path: 'XLM → USDC',
  pct: 100,
};

const SPLIT_ROUTES = [
  { dex: 'Aquarius CLMM', path: 'XLM → USDC', pct: 58 },
  { dex: 'Soroswap → Aquarius', path: 'XLM → AQUA → USDC', pct: 42 },
] as const;

export function CompareSection() {
  return (
    <section className="space-y-6">
      <div className="text-center md:text-left space-y-2">
        <p className="text-[11px] uppercase tracking-widest text-blue-400/90 font-medium">
          Why aggregate
        </p>
        <h2 className="text-xl md:text-2xl font-semibold text-slate-100 tracking-tight">
          One DEX vs. split routing
        </h2>
        <p className="text-sm text-slate-400 max-w-xl">
          Same trade size, different execution. LumAgg can send volume through several venues when
          that beats relying on a single pool.
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Single DEX */}
        <div className="rounded-2xl border border-white/10 bg-slate-900/40 p-5 flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-slate-400">Single DEX</span>
            <span className="text-[10px] text-slate-500">Example only</span>
          </div>
          <div>
            <div className="text-[11px] text-slate-500 mb-1">{EXAMPLE_IN} → USDC</div>
            <div className="text-2xl font-semibold text-slate-300 tabular-nums">
              {SINGLE_OUT} <span className="text-sm font-normal text-slate-500">USDC</span>
            </div>
          </div>
          <div className="space-y-2">
            <div className="h-2 rounded-full bg-slate-800 overflow-hidden">
              <div className="h-full w-full bg-slate-600 rounded-full" />
            </div>
            <div className="flex justify-between text-[11px] text-slate-400">
              <span>{SINGLE_ROUTE.dex}</span>
              <span>{SINGLE_ROUTE.pct}%</span>
            </div>
            <div className="text-[10px] text-slate-500">{SINGLE_ROUTE.path}</div>
          </div>
          <p className="text-[11px] text-slate-500 leading-relaxed mt-auto">
            Stuck with whatever depth that one venue has at this block.
          </p>
        </div>

        {/* LumAgg split */}
        <div className="rounded-2xl border border-blue-500/30 bg-gradient-to-br from-blue-500/10 via-slate-900/60 to-violet-500/10 p-5 flex flex-col gap-4">
          <div className="flex items-start justify-between gap-3">
            <span className="text-xs font-medium text-blue-300 shrink-0">LumAgg split</span>
            <div className="flex flex-col items-end gap-1 min-w-0">
              <span className="rounded-full bg-emerald-500/15 border border-emerald-400/30 px-2 py-0.5 text-[10px] font-medium text-emerald-300 whitespace-nowrap">
                +{IMPROVEMENT_PCT}% output
              </span>
              <span className="text-[10px] text-slate-500">Example only</span>
            </div>
          </div>
          <div>
            <div className="text-[11px] text-slate-500 mb-1">{EXAMPLE_IN} → USDC</div>
            <div className="text-2xl font-semibold text-white tabular-nums">
              {SPLIT_OUT} <span className="text-sm font-normal text-slate-400">USDC</span>
            </div>
          </div>
          <div className="space-y-3">
            {SPLIT_ROUTES.map((r) => (
              <div key={r.dex} className="space-y-1">
                <div className="h-2 rounded-full bg-slate-800 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-cyan-500/80 to-blue-500/80"
                    style={{ width: `${r.pct}%` }}
                  />
                </div>
                <div className="flex justify-between text-[11px]">
                  <span className="text-slate-300">{r.dex}</span>
                  <span className="text-slate-500 font-mono">{r.pct}%</span>
                </div>
                <div className="text-[10px] text-slate-500">{r.path}</div>
              </div>
            ))}
          </div>
          <p className="text-[11px] text-slate-400 leading-relaxed mt-auto">
            One transaction, multiple paths — sized by live quotes and your slippage limit.
          </p>
        </div>
      </div>

      <p className="text-center text-[10px] text-slate-600">
        Illustrative amounts for 10 XLM → USDC. Your actual quote updates in the swap widget above.
      </p>
    </section>
  );
}

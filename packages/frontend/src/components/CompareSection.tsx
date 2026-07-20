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
    <section className="space-y-6 pt-4 border-t border-white/[0.06]">
      <div className="space-y-2">
        <p className="section-label">Why aggregate</p>
        <h2 className="section-title md:text-xl">One DEX vs. split routing</h2>
        <p className="text-[13px] text-zinc-400 max-w-xl leading-relaxed">
          Same trade size, different execution. LumAgg can send volume through several venues when
          that beats relying on a single pool.
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div className="surface-panel p-5 flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <span className="text-[13px] font-medium text-zinc-400">Single DEX</span>
            <span className="text-[11px] text-zinc-600">Example only</span>
          </div>
          <div>
            <div className="text-[12px] text-zinc-500 mb-1">{EXAMPLE_IN} → USDC</div>
            <div className="text-2xl font-semibold text-zinc-300 tabular-nums tracking-tight">
              {SINGLE_OUT} <span className="text-sm font-normal text-zinc-500">USDC</span>
            </div>
          </div>
          <div className="space-y-2">
            <div className="h-1.5 rounded-full bg-zinc-800 overflow-hidden">
              <div className="h-full w-full bg-zinc-600 rounded-full" />
            </div>
            <div className="flex justify-between text-[12px] text-zinc-400">
              <span>{SINGLE_ROUTE.dex}</span>
              <span>{SINGLE_ROUTE.pct}%</span>
            </div>
            <div className="text-[11px] text-zinc-600">{SINGLE_ROUTE.path}</div>
          </div>
          <p className="text-[12px] text-zinc-500 leading-relaxed mt-auto">
            Stuck with whatever depth that one venue has at this block.
          </p>
        </div>

        <div className="surface-panel p-5 flex flex-col gap-4 ring-1 ring-white/[0.06]">
          <div className="flex items-start justify-between gap-3">
            <span className="text-[13px] font-medium text-zinc-200 shrink-0">LumAgg split</span>
            <div className="flex flex-col items-end gap-1 min-w-0">
              <span className="rounded-md bg-emerald-500/10 border border-emerald-500/20 px-2 py-0.5 text-[11px] font-medium text-emerald-400 whitespace-nowrap">
                +{IMPROVEMENT_PCT}% output
              </span>
              <span className="text-[11px] text-zinc-600">Example only</span>
            </div>
          </div>
          <div>
            <div className="text-[12px] text-zinc-500 mb-1">{EXAMPLE_IN} → USDC</div>
            <div className="text-2xl font-semibold text-zinc-50 tabular-nums tracking-tight">
              {SPLIT_OUT} <span className="text-sm font-normal text-zinc-400">USDC</span>
            </div>
          </div>
          <div className="space-y-3">
            {SPLIT_ROUTES.map((r) => (
              <div key={r.dex} className="space-y-1">
                <div className="h-1.5 rounded-full bg-zinc-800 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-blue-500/70"
                    style={{ width: `${r.pct}%` }}
                  />
                </div>
                <div className="flex justify-between text-[12px]">
                  <span className="text-zinc-300">{r.dex}</span>
                  <span className="text-zinc-500 font-mono">{r.pct}%</span>
                </div>
                <div className="text-[11px] text-zinc-600">{r.path}</div>
              </div>
            ))}
          </div>
          <p className="text-[12px] text-zinc-400 leading-relaxed mt-auto">
            One transaction, multiple paths — sized by live quotes and your slippage limit.
          </p>
        </div>
      </div>

      <p className="text-center text-[11px] text-zinc-600">
        Illustrative amounts for 10 XLM → USDC. Your actual quote updates in the swap widget above.
      </p>
    </section>
  );
}

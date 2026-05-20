'use client';

import { SwapCard } from '@/components/SwapCard';

export default function Home() {
  return (
    <div className="w-full space-y-10">
      <section className="grid grid-cols-1 lg:grid-cols-[1.1fr_0.9fr] gap-8 items-start">
        <div className="space-y-6">
          <div className="inline-flex items-center gap-2 rounded-full border border-emerald-400/30 bg-emerald-500/10 px-3 py-1 text-xs text-emerald-200">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-300" />
            Mainnet live
          </div>

          <div className="space-y-4">
            <h1 className="text-4xl md:text-6xl font-semibold tracking-tight leading-[1.05] bg-gradient-to-br from-white via-slate-200 to-blue-300 bg-clip-text text-transparent">
              Professional-grade
              <br />
              Stellar execution
            </h1>
            <p className="text-slate-300/90 text-base md:text-lg leading-relaxed max-w-xl">
              LumAgg routes every swap across major Stellar DEX liquidity, compares paths in real
              time, and returns a wallet-ready transaction with slippage protection.
            </p>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 max-w-2xl">
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-4">
              <div className="text-2xl font-semibold text-white">6</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">DEX venues</div>
            </div>
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-4">
              <div className="text-2xl font-semibold text-white">500+</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">Pools tracked</div>
            </div>
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-4">
              <div className="text-2xl font-semibold text-white">&lt;100ms</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">Quote response</div>
            </div>
          </div>

          <div className="rounded-2xl border border-white/10 bg-gradient-to-r from-blue-500/10 to-indigo-500/10 p-4">
            <div className="text-sm text-slate-200 font-medium">How routing works</div>
            <div className="mt-3 grid grid-cols-1 md:grid-cols-3 gap-2 text-xs text-slate-300">
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-3">1) Scan all connected pools</div>
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-3">2) Evaluate best path by net output</div>
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-3">3) Build tx with min received guard</div>
            </div>
          </div>
        </div>

        <div className="relative">
          <div className="absolute -inset-4 bg-blue-500/10 blur-2xl rounded-[2rem] pointer-events-none" />
          <div className="relative rounded-[1.75rem] border border-white/10 bg-slate-900/40 p-3">
            <SwapCard />
          </div>
        </div>
      </section>

      <section className="rounded-2xl border border-white/10 bg-slate-900/40 p-5">
        <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
          <h2 className="text-sm md:text-base font-medium text-slate-100">Liquidity Coverage</h2>
          <span className="text-xs text-slate-400">Aquarius · Soroswap · Phoenix · Sushi · Comet · Classic</span>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-6 gap-2">
          {['Aquarius', 'Soroswap', 'Phoenix', 'Sushi', 'Comet', 'Classic'].map((dex) => (
            <div
              key={dex}
              className="rounded-xl border border-white/10 bg-slate-950/70 px-3 py-2 text-center text-xs text-slate-300"
            >
              {dex}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

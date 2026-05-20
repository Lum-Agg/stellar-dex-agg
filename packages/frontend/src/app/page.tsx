'use client';

import { SwapCard } from '@/components/SwapCard';

const FEATURES = [
  {
    title: 'Smart routing',
    body: 'Compares paths across multiple DEX venues and selects the route with the best expected output.',
  },
  {
    title: 'Slippage guard',
    body: 'Every quote includes minimum received so wallets sign transactions with explicit protection.',
  },
  {
    title: 'Wallet-ready txs',
    body: 'Simulation assembles Soroban footprints and auth so you can sign and submit in one flow.',
  },
] as const;

const DEXES = ['Aquarius', 'Soroswap', 'Phoenix', 'Sushi', 'Comet', 'Classic'] as const;

export default function Home() {
  return (
    <div className="w-full space-y-8 md:space-y-10 pb-6">
      <section className="grid grid-cols-1 lg:grid-cols-[1.05fr_0.95fr] gap-8 lg:gap-10 items-start">
        <div className="space-y-6">
          <div className="inline-flex w-fit items-center gap-2 rounded-full border border-emerald-400/30 bg-emerald-500/10 px-3 py-1 text-xs text-emerald-200">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-300 animate-pulse" />
            Mainnet live
          </div>

          <div className="space-y-4">
            <h1 className="text-4xl md:text-5xl xl:text-6xl font-semibold tracking-tight leading-[1.05] bg-gradient-to-br from-white via-slate-200 to-blue-300 bg-clip-text text-transparent">
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
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-5">
              <div className="text-2xl font-semibold text-white">6</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">DEX venues</div>
            </div>
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-5">
              <div className="text-2xl font-semibold text-white">500+</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">Pools tracked</div>
            </div>
            <div className="rounded-2xl border border-white/10 bg-slate-900/60 px-4 py-5">
              <div className="text-2xl font-semibold text-white">&lt;100ms</div>
              <div className="text-[11px] mt-1 text-slate-400 uppercase tracking-wider">Quote response</div>
            </div>
          </div>

          <div className="rounded-2xl border border-white/10 bg-gradient-to-r from-blue-500/10 to-indigo-500/10 p-5">
            <div className="text-sm text-slate-200 font-medium mb-3">How routing works</div>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-2 text-xs text-slate-300">
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-4">1) Scan all connected pools</div>
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-4">2) Pick best path by net output</div>
              <div className="rounded-xl border border-white/10 bg-slate-900/60 p-4">3) Build tx with min received</div>
            </div>
          </div>
        </div>

        <div className="relative lg:sticky lg:top-24">
          <div className="absolute -inset-6 bg-blue-500/10 blur-3xl rounded-[2rem] pointer-events-none" />
          <div className="relative rounded-[1.75rem] border border-white/10 bg-slate-900/50 p-3 md:p-4 shadow-2xl shadow-black/40">
            <SwapCard />
          </div>
        </div>
      </section>

      <section className="grid grid-cols-1 md:grid-cols-3 gap-3">
        {FEATURES.map((f) => (
          <div
            key={f.title}
            className="rounded-2xl border border-white/10 bg-slate-900/40 p-4 flex flex-col gap-2"
          >
            <h3 className="text-sm font-medium text-slate-100">{f.title}</h3>
            <p className="text-xs text-slate-400 leading-relaxed">{f.body}</p>
          </div>
        ))}
      </section>

      <section className="rounded-2xl border border-white/10 bg-slate-900/40 p-5">
        <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
          <h2 className="text-sm md:text-base font-medium text-slate-100">Liquidity coverage</h2>
          <span className="text-xs text-slate-400">Unified routing across Stellar DEX venues</span>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-6 gap-2">
          {DEXES.map((dex) => (
            <div
              key={dex}
              className="rounded-xl border border-white/10 bg-slate-950/70 px-3 py-3 text-center text-xs text-slate-300"
            >
              {dex}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

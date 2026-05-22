'use client';

import { SwapCard } from '@/components/SwapCard';
import { CompareSection } from '@/components/CompareSection';
import { FaqSection } from '@/components/FaqSection';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';

const VENUES = [
  { name: 'Aquarius', tag: 'CLMM + AMM' },
  { name: 'Soroswap', tag: 'AMM' },
  { name: 'Phoenix', tag: 'AMM' },
  { name: 'Sushi', tag: 'V3' },
  { name: 'Comet', tag: 'Weighted' },
  { name: 'Classic', tag: 'SDEX' },
] as const;

const STEPS = [
  { n: '01', title: 'Enter amount', body: 'Pick tokens and size. We fetch live quotes from every connected venue.' },
  { n: '02', title: 'Review route', body: 'See single- or multi-path execution, price impact, and minimum received.' },
  { n: '03', title: 'Sign once', body: 'Wallet signs a single Soroban tx that executes the aggregated route on-chain.' },
] as const;

export default function Home() {
  return (
    <div className="w-full space-y-14 md:space-y-20 pb-10">
      {/* ——— Swap (unchanged focus) ——— */}
      <section className="flex flex-col items-center pt-1 md:pt-2 w-full max-w-[440px] mx-auto">
        <div className="inline-flex w-fit items-center gap-2 rounded-full border border-emerald-400/30 bg-emerald-500/10 px-3 py-1 text-xs text-emerald-200 mb-4">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-300 animate-pulse" />
          Mainnet live
        </div>
        <DisclaimerBanner className="mb-5 w-full" />
        <div className="relative w-full">
          <div className="absolute -inset-8 bg-blue-500/15 blur-3xl rounded-[2rem] pointer-events-none" />
          <div className="relative rounded-[1.75rem] border border-white/10 bg-slate-900/60 p-3 md:p-4 shadow-2xl shadow-black/40">
            <SwapCard />
          </div>
        </div>
        <p className="mt-4 text-center text-[11px] text-slate-500 max-w-sm">
          Best-effort routing across 6 Stellar DEXs · Quotes in under a second
        </p>
      </section>

      {/* ——— How it works (compact) ——— */}
      <section className="space-y-6">
        <h2 className="text-center text-sm font-medium text-slate-400 uppercase tracking-wider">
          Three steps
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {STEPS.map((s) => (
            <div
              key={s.n}
              className="rounded-2xl border border-white/10 bg-slate-900/30 p-4 md:p-5"
            >
              <div className="text-[10px] font-mono text-blue-400/80 mb-2">{s.n}</div>
              <h3 className="text-sm font-medium text-slate-100 mb-1.5">{s.title}</h3>
              <p className="text-xs text-slate-400 leading-relaxed">{s.body}</p>
            </div>
          ))}
        </div>
      </section>

      <CompareSection />

      {/* ——— Venues ——— */}
      <section className="space-y-4">
        <h2 className="text-sm font-medium text-slate-300">Liquidity sources</h2>
        <div className="flex flex-wrap gap-2">
          {VENUES.map((v) => (
            <div
              key={v.name}
              className="rounded-xl border border-white/10 bg-slate-950/60 px-3 py-2 flex items-baseline gap-2"
            >
              <span className="text-xs font-medium text-slate-200">{v.name}</span>
              <span className="text-[10px] text-slate-500">{v.tag}</span>
            </div>
          ))}
        </div>
      </section>

      <FaqSection />

      {/* ——— CTA back to swap ——— */}
      <section className="rounded-2xl border border-white/10 bg-gradient-to-r from-blue-600/10 to-violet-600/10 px-5 py-6 text-center">
        <p className="text-sm text-slate-300 mb-1">Ready to try a swap?</p>
        <p className="text-xs text-slate-500 mb-4">Scroll up — connect your wallet and get a live quote.</p>
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            window.scrollTo({ top: 0, behavior: 'smooth' });
          }}
          className="inline-flex items-center justify-center rounded-xl bg-gradient-to-r from-blue-600 to-violet-600 px-5 py-2.5 text-sm font-medium text-white hover:opacity-90 transition-opacity"
        >
          Back to swap
        </a>
      </section>
    </div>
  );
}

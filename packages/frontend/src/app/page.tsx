'use client';

import { SwapCard } from '@/components/SwapCard';
import { CompareSection } from '@/components/CompareSection';
import { FaqSection } from '@/components/FaqSection';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';

const VENUES = [
  { name: 'Aquarius', tag: 'AMM · Stable · CLMM' },
  { name: 'Soroswap', tag: 'AMM' },
  { name: 'Phoenix', tag: 'AMM' },
  { name: 'Sushi', tag: 'CLMM' },
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
    <div className="w-full space-y-16 md:space-y-20 pb-12">
      <section className="flex flex-col items-center w-full max-w-[420px] mx-auto">
        <div className="inline-flex w-fit items-center gap-2 rounded-md border border-white/[0.08] bg-zinc-900/80 px-2.5 py-1 text-[11px] text-zinc-400 mb-5">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
          Mainnet live
        </div>
        <DisclaimerBanner className="mb-5 w-full" />
        <SwapCard />
        <p className="mt-4 text-center text-[12px] text-zinc-500 max-w-sm leading-relaxed">
          Best-effort routing across 6 Stellar DEXs · Quotes in under a second
        </p>
      </section>

      <section className="space-y-6 pt-4 border-t border-white/[0.06]">
        <h2 className="section-label text-center sm:text-left">Three steps</h2>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {STEPS.map((s) => (
            <div
              key={s.n}
              className="surface-panel p-5"
            >
              <div className="text-[11px] font-mono text-zinc-500 mb-2">{s.n}</div>
              <h3 className="text-sm font-medium text-zinc-100 mb-1.5">{s.title}</h3>
              <p className="text-[13px] text-zinc-400 leading-relaxed">{s.body}</p>
            </div>
          ))}
        </div>
      </section>

      <CompareSection />

      <section className="space-y-4 pt-4 border-t border-white/[0.06]">
        <h2 className="section-title">Liquidity sources</h2>
        <div className="flex flex-wrap gap-2">
          {VENUES.map((v) => (
            <div
              key={v.name}
              className="rounded-md border border-white/[0.08] bg-zinc-900/50 px-3 py-2 flex items-baseline gap-2"
            >
              <span className="text-[13px] font-medium text-zinc-200">{v.name}</span>
              <span className="text-[11px] text-zinc-500">{v.tag}</span>
            </div>
          ))}
        </div>
      </section>

      <FaqSection />

      <section className="surface-panel px-5 py-6 text-center">
        <p className="text-sm text-zinc-300 mb-1">Ready to try a swap?</p>
        <p className="text-[13px] text-zinc-500 mb-4">Scroll up — connect your wallet and get a live quote.</p>
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            window.scrollTo({ top: 0, behavior: 'smooth' });
          }}
          className="btn-primary inline-flex px-5 py-2.5 text-sm"
        >
          Back to swap
        </a>
      </section>
    </div>
  );
}

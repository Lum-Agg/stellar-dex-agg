'use client';

import { useState } from 'react';
import Link from 'next/link';
import dynamic from 'next/dynamic';
import { SwapCard } from '@/components/SwapCard';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';
import { SwapHistory } from '@/components/SwapHistory';
import { OrderTypeRail } from '@/components/OrderTypeRail';
import { GITHUB_REPO_URL } from '@/lib/site';

const SELF_HOST_URL = `${GITHUB_REPO_URL}/blob/main/docs/self-hosted-aggregator-quickstart.md`;

function OrderCardFallback() {
  return (
    <div className="surface-panel h-[30rem] animate-pulse p-6" aria-label="Loading order form">
      <div className="h-5 w-24 rounded bg-white/[0.06]" />
      <div className="mt-8 h-32 rounded-xl bg-white/[0.04]" />
      <div className="mt-4 h-32 rounded-xl bg-white/[0.04]" />
      <div className="mt-5 h-14 rounded-xl bg-white/[0.05]" />
    </div>
  );
}

const LimitCard = dynamic(
  () => import('@/components/LimitCard').then((module) => module.LimitCard),
  { loading: () => <OrderCardFallback /> },
);

const DcaCard = dynamic(() => import('@/components/DcaCard').then((module) => module.DcaCard), {
  loading: () => <OrderCardFallback />,
});

export default function Home() {
  const [orderType, setOrderType] = useState<'instant' | 'limit' | 'dca'>('instant');

  return (
    <div className="w-full max-w-5xl mx-auto">
      <header className="mb-8 md:mb-10 flex flex-col md:flex-row md:items-end md:justify-between gap-5 border-b border-[var(--border)] pb-6">
        <div className="max-w-2xl">
          <span className="eyebrow">Open-source Stellar routing</span>
          <h1 className="mt-3 text-3xl sm:text-4xl font-semibold tracking-[-0.04em] text-[var(--text-primary)]">
            Better execution across Stellar DEXes.
          </h1>
          <p className="mt-3 max-w-xl text-[14px] sm:text-[15px] leading-relaxed text-[var(--text-secondary)]">
            Compare routes across Soroban AMMs and Classic SDEX, then review and sign the unsigned
            transaction in your own wallet.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-3 text-[13px] font-medium">
          <Link
            href="/docs"
            className="rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3.5 py-2 text-[var(--text-secondary)] hover:border-[var(--border-strong)] hover:text-[var(--text-primary)] transition-colors"
          >
            Integrate the API
          </Link>
          <a
            href={SELF_HOST_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-lg border border-[var(--accent)]/30 bg-[var(--accent)]/[0.06] px-3.5 py-2 text-[var(--accent)] hover:bg-[var(--accent)]/[0.1] transition-colors"
          >
            Self-host the stack ↗
          </a>
        </div>
      </header>

      <div className="flex flex-col sm:flex-row gap-5 sm:gap-8 lg:gap-12 items-stretch sm:items-start justify-start sm:justify-center pt-1 sm:pt-2 md:pt-4 w-full">
        <OrderTypeRail active={orderType} onSelect={setOrderType} />

        <div className="w-full sm:w-[520px] shrink-0 min-w-0">
          <DisclaimerBanner className="mb-3" />
          {orderType === 'instant' ? (
            <>
              <SwapCard />
              <div className="mt-4 w-full">
                <SwapHistory />
              </div>
            </>
          ) : orderType === 'limit' ? (
            <LimitCard />
          ) : (
            <DcaCard />
          )}
          <Link
            href="/portfolio"
            className="home-secondary-link mt-4 inline-block text-[13px] sm:text-[14px] text-[var(--text-muted)] hover:text-[var(--accent)] transition-colors"
          >
            View portfolio →
          </Link>
        </div>
      </div>
    </div>
  );
}

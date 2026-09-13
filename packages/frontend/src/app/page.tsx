'use client';

import { useState } from 'react';
import Link from 'next/link';
import { SwapCard } from '@/components/SwapCard';
import { LimitCard } from '@/components/LimitCard';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';
import { SwapHistory } from '@/components/SwapHistory';
import { OrderTypeRail } from '@/components/OrderTypeRail';
import { DcaCard } from '@/components/DcaCard';

export default function Home() {
  const [orderType, setOrderType] = useState<'instant' | 'limit' | 'dca'>('instant');

  return (
    <div className="w-full">
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

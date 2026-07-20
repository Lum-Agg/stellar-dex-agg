'use client';

import { useState } from 'react';

const FAQ = [
  {
    q: 'What is LumAgg?',
    a: 'LumAgg is a Stellar DEX aggregator. It scans liquidity on Aquarius, Soroswap, Phoenix, Sushi, Comet, and the Stellar Classic DEX, then returns the best route and a wallet-ready Soroban transaction.',
  },
  {
    q: 'What is split routing?',
    a: 'For large trades, a single pool may have poor depth. LumAgg can divide your swap across multiple paths (e.g. 60% on one DEX, 40% on another) so total output is higher than using one venue alone.',
  },
  {
    q: 'Do you hold my funds?',
    a: 'No. LumAgg is non-custodial. You connect your wallet, review the transaction, sign locally, and submit to the network. We only provide quotes and transaction building.',
  },
  {
    q: 'Which wallets are supported?',
    a: 'Any Stellar wallet that supports Soroban transactions and signing — for example Freighter, xBull, or LOBSTR (via Wallet Standard).',
  },
  {
    q: 'What is slippage tolerance?',
    a: 'It is the maximum price movement you accept before the swap fails. The quote includes a minimum received amount based on your chosen slippage (0.1%, 0.5%, or 1%).',
  },
  {
    q: 'Is there an API?',
    a: 'Yes. See API Docs in the header for quote and build_tx endpoints so you can integrate routing into your own app.',
  },
  {
    q: 'Is this on mainnet?',
    a: 'Yes. LumAgg runs against Stellar mainnet liquidity. Always verify token contracts and amounts before signing.',
  },
  {
    q: 'Is it production-ready?',
    a: 'The app is live on mainnet but still being optimized. Use small sizes first, double-check quotes, and treat routing as best-effort until you are comfortable with the risks.',
  },
] as const;

export function FaqSection() {
  const [open, setOpen] = useState<number | null>(0);

  return (
    <section className="space-y-5 pt-4 border-t border-white/[0.06]">
      <div className="space-y-2">
        <p className="section-label">FAQ</p>
        <h2 className="section-title md:text-xl">Common questions</h2>
      </div>

      <div className="surface-panel divide-y divide-white/[0.06] overflow-hidden">
        {FAQ.map((item, i) => {
          const isOpen = open === i;
          return (
            <div key={item.q}>
              <button
                type="button"
                onClick={() => setOpen(isOpen ? null : i)}
                className="w-full flex items-center justify-between gap-4 px-4 md:px-5 py-4 text-left hover:bg-white/[0.02] transition-colors"
                aria-expanded={isOpen}
              >
                <span className="text-[13px] font-medium text-zinc-200">{item.q}</span>
                <span
                  className={`shrink-0 text-zinc-500 text-lg leading-none transition-transform ${isOpen ? 'rotate-45' : ''}`}
                  aria-hidden
                >
                  +
                </span>
              </button>
              {isOpen && (
                <div className="px-4 md:px-5 pb-4 text-[13px] text-zinc-400 leading-relaxed -mt-1">
                  {item.a}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

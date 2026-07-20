'use client';

import { TokenIcon, type Token } from '@/components/TokenSelector';
import { formatBalanceDisplay } from '@/lib/balance';
import { Sparkline } from '@/components/Sparkline';
import type { PriceHistoryPoint } from '@/lib/prices';

export interface ValuedHolding {
  id: string;
  balance: bigint;
  decimals: number;
  symbol: string;
  price: number | null;
  value: number | null;
  history: PriceHistoryPoint[];
  token?: Token;
}

function formatUsd(value: number | null, maximumFractionDigits = 2): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return value.toLocaleString(undefined, {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: value >= 1 ? maximumFractionDigits : 6,
  });
}

function shortContractId(contractId: string): string {
  return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
}

export function HoldingsTable({ holdings }: { holdings: ValuedHolding[] }) {
  if (holdings.length === 0) {
    return (
      <div className="surface-panel px-5 py-10 text-center">
        <h2 className="text-[15px] font-medium text-[var(--text-primary)]">
          No token balances yet
        </h2>
        <p className="mt-1 text-[13px] text-[var(--text-muted)]">
          Your non-zero Stellar token balances will appear here.
        </p>
      </div>
    );
  }

  return (
    <section className="overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]/60">
      <div className="overflow-x-auto">
        <table className="w-full min-w-[640px] text-left text-[14px]">
          <thead className="border-b border-[var(--border)] bg-[var(--bg-0)]/40 text-[12px] uppercase tracking-wide text-[var(--text-muted)]">
            <tr>
              <th className="px-4 py-3 font-medium sm:px-5">Asset</th>
              <th className="px-3 py-3 text-right font-medium">Balance</th>
              <th className="px-3 py-3 text-right font-medium">Price</th>
              <th className="px-4 py-3 text-right font-medium sm:px-5">Value</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--border)]">
            {holdings.map((holding) => (
              <tr key={holding.id} className="transition-colors hover:bg-white/[0.02]">
                <td className="px-4 py-3.5 sm:px-5">
                  <div className="flex items-start gap-3">
                    {holding.token ? (
                      <TokenIcon token={holding.token} size={32} />
                    ) : (
                      <div
                        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--surface-raised)] text-[12px] font-semibold text-[var(--text-secondary)]"
                        aria-hidden
                      >
                        {holding.symbol[0]}
                      </div>
                    )}
                    <div className="min-w-0">
                      <div className="font-medium text-[var(--text-primary)]">{holding.symbol}</div>
                      <div className="mt-0.5 max-w-[200px] truncate font-[family-name:var(--font-mono)] text-[11px] text-[var(--text-muted)]">
                        {shortContractId(holding.id)}
                      </div>
                      <div className="mt-2">
                        <Sparkline points={holding.history} />
                      </div>
                    </div>
                  </div>
                </td>
                <td className="px-3 py-3.5 text-right tabular-nums text-[var(--text-secondary)]">
                  {formatBalanceDisplay(holding.balance, holding.decimals)}
                </td>
                <td className="px-3 py-3.5 text-right tabular-nums text-[var(--text-muted)]">
                  {formatUsd(holding.price, 6)}
                </td>
                <td className="px-4 py-3.5 text-right font-medium tabular-nums text-[var(--text-primary)] sm:px-5">
                  {formatUsd(holding.value)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

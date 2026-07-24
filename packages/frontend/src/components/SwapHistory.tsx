'use client';

import { useMemo } from 'react';
import { formatBalanceDisplay } from '@/lib/balance';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { useSwapHistory } from '@/lib/useSwapHistory';
import { useWallet } from '@/lib/wallet-context';
import { useTokenList } from './TokenSelector';

function relativeTime(timestamp: number): string {
  const date = timestamp < 1_000_000_000_000 ? timestamp * 1000 : timestamp;
  const seconds = Math.max(0, Math.floor((Date.now() - date) / 1000));

  if (seconds < 60) return 'just now';
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h ago`;
  if (seconds < 604_800) return `${Math.floor(seconds / 86_400)}d ago`;
  return new Date(date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function shortContractId(contractId: string): string {
  return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
}

type SwapHistoryProps = {
  variant?: 'compact' | 'profile';
};

export function SwapHistory({ variant = 'compact' }: SwapHistoryProps) {
  const { address, connect, connecting } = useWallet();
  const tokens = useTokenList();
  const { swaps, loading, loadingMore, hasMore, loadMore, unavailable, refetchError } =
    useSwapHistory();

  const tokenById = useMemo(() => new Map(tokens.map((token) => [token.id, token])), [tokens]);

  const tokenLabel = (contractId: string | null) => {
    if (!contractId) return 'Unknown';
    const token = tokenById.get(contractId);
    if (token) return displayTokenSymbol(token.symbol, contractId);
    if (contractId === NATIVE_CONTRACT || contractId === 'native') return 'XLM';
    return shortContractId(contractId);
  };

  const amountLabel = (amount: string | null, contractId: string | null) => {
    if (amount === null) return '—';
    try {
      const decimals = tokenById.get(contractId ?? '')?.decimals ?? 7;
      return formatBalanceDisplay(BigInt(amount), decimals);
    } catch {
      return amount;
    }
  };

  if (!address) {
    if (variant === 'profile') return null;
    return (
      <section className="rounded-2xl border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3.5">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-[15px] font-medium text-[var(--text-secondary)]">Activity</h2>
            <p className="mt-0.5 text-[13px] text-[var(--text-muted)]">
              Connect to see recent swaps
            </p>
          </div>
          <button
            type="button"
            onClick={connect}
            disabled={connecting}
            className="shrink-0 rounded-lg border border-[var(--border)] px-3.5 py-2 text-[13px] text-[var(--text-secondary)] hover:border-[var(--accent)]/40 hover:text-[var(--accent)] disabled:opacity-50 transition-colors"
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </button>
        </div>
      </section>
    );
  }

  if (variant === 'profile') {
    return (
      <section className="overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]/60">
        {loading && swaps.length === 0 ? (
          <div className="grid gap-2 p-4">
            {Array.from({ length: 4 }).map((_, index) => (
              <div key={index} className="h-12 animate-pulse rounded-xl bg-[var(--bg-0)]/60" />
            ))}
          </div>
        ) : unavailable ? (
          <p className="px-4 py-6 text-[14px] text-[var(--text-muted)]">History unavailable</p>
        ) : swaps.length === 0 ? (
          <p className="px-4 py-6 text-[14px] text-[var(--text-muted)]">No swaps yet</p>
        ) : (
          <>
            {refetchError && (
              <p className="px-4 py-2 text-[12px] text-amber-400/80 border-b border-[var(--border)]">
                Couldn&apos;t refresh history
              </p>
            )}
            <div className="overflow-x-auto">
              <table className="w-full min-w-[640px] text-left text-[14px]">
                <thead className="border-b border-[var(--border)] bg-[var(--bg-0)]/40 text-[12px] uppercase tracking-wide text-[var(--text-muted)]">
                  <tr>
                    <th className="px-4 py-3 font-medium sm:px-5">Time</th>
                    <th className="px-3 py-3 font-medium">Status</th>
                    <th className="px-3 py-3 font-medium">Swap</th>
                    <th className="px-4 py-3 text-right font-medium sm:px-5">Tx</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--border)]">
                  {swaps.map((swap) => (
                    <tr
                      key={`${swap.tx_hash}-${swap.ledger}`}
                      className="hover:bg-white/[0.02] transition-colors"
                    >
                      <td className="px-4 py-3 text-[var(--text-muted)] sm:px-5">
                        {relativeTime(swap.created_at)}
                      </td>
                      <td className="px-3 py-3">
                        <span
                          className={
                            swap.status === 'SUCCESS'
                              ? 'text-[var(--accent)]'
                              : 'text-[var(--text-muted)]'
                          }
                        >
                          {swap.status}
                        </span>
                      </td>
                      <td className="px-3 py-3 tabular-nums font-[family-name:var(--font-mono)]">
                        <span className="text-[var(--text-secondary)]">
                          {amountLabel(swap.amount_in, swap.token_in)} {tokenLabel(swap.token_in)}
                        </span>
                        <span className="mx-1.5 text-[var(--text-muted)]">→</span>
                        <span className="text-[var(--text-primary)]">
                          {amountLabel(swap.amount_out, swap.token_out)}{' '}
                          {tokenLabel(swap.token_out)}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-right sm:px-5">
                        <a
                          href={`https://stellar.expert/explorer/public/tx/${swap.tx_hash}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-[13px] text-[var(--text-secondary)] hover:text-[var(--accent)] transition-colors"
                        >
                          View
                        </a>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {loading && (
              <p className="border-t border-[var(--border)] px-4 py-2 text-[12px] text-[var(--text-muted)]">
                Refreshing…
              </p>
            )}
            {hasMore && (
              <div className="border-t border-[var(--border)] px-4 py-3">
                <button
                  type="button"
                  onClick={() => void loadMore()}
                  disabled={loadingMore}
                  className="w-full rounded-xl border border-[var(--border)] px-4 py-2.5 text-[13px] text-[var(--text-secondary)] hover:border-[var(--accent)]/40 hover:text-[var(--accent)] disabled:opacity-50 transition-colors"
                >
                  {loadingMore ? 'Loading…' : 'Load more'}
                </button>
              </div>
            )}
          </>
        )}
      </section>
    );
  }

  return (
    <section className="rounded-2xl border border-[var(--border)] bg-[var(--surface)]/60 overflow-hidden max-h-64 overflow-y-auto">
      <div className="sticky top-0 flex items-center justify-between px-4 py-2.5 border-b border-[var(--border)] bg-[var(--surface)]/95 backdrop-blur-sm">
        <h2 className="text-[15px] font-medium text-[var(--text-secondary)]">Activity</h2>
        {loading && <span className="text-[13px] text-[var(--text-muted)]">Loading…</span>}
      </div>

      {unavailable ? (
        <p className="px-4 py-3 text-[13px] text-[var(--text-muted)]">History unavailable</p>
      ) : swaps.length === 0 && !loading ? (
        <p className="px-4 py-3 text-[13px] text-[var(--text-muted)]">No swaps yet</p>
      ) : (
        <>
          {refetchError && (
            <p className="px-4 py-1.5 text-[12px] text-amber-400/80 border-b border-[var(--border)]">
              Couldn&apos;t refresh history
            </p>
          )}
          <div className="divide-y divide-[var(--border)]">
            {swaps.map((swap) => (
              <a
                key={`${swap.tx_hash}-${swap.ledger}`}
                href={`https://stellar.expert/explorer/public/tx/${swap.tx_hash}`}
                target="_blank"
                rel="noopener noreferrer"
                className="block px-4 py-2.5 hover:bg-white/[0.02] transition-colors"
              >
                <div className="flex items-center justify-between gap-3">
                  <span className="text-[12px] text-[var(--text-muted)]">
                    {relativeTime(swap.created_at)}
                  </span>
                  <span
                    className={
                      swap.status === 'SUCCESS'
                        ? 'text-[12px] text-[var(--accent)]'
                        : 'text-[12px] text-[var(--text-muted)]'
                    }
                  >
                    {swap.status}
                  </span>
                </div>
                <div className="mt-0.5 flex items-center gap-1.5 text-[14px] tabular-nums font-[family-name:var(--font-mono)]">
                  <span className="text-[var(--text-secondary)]">
                    {amountLabel(swap.amount_in, swap.token_in)} {tokenLabel(swap.token_in)}
                  </span>
                  <span className="text-[var(--text-muted)]">→</span>
                  <span className="text-[var(--text-primary)]">
                    {amountLabel(swap.amount_out, swap.token_out)} {tokenLabel(swap.token_out)}
                  </span>
                </div>
              </a>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

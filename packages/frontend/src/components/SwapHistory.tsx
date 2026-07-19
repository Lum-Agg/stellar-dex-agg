'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { formatBalanceDisplay } from '@/lib/balance';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { fetchUserSwaps, SWAP_SUCCESS_EVENT, type UserSwap } from '@/lib/swaps';
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

export function SwapHistory() {
  const { address, connect, connecting } = useWallet();
  const tokens = useTokenList();
  const [swaps, setSwaps] = useState<UserSwap[]>([]);
  const [loading, setLoading] = useState(false);
  const [unavailable, setUnavailable] = useState(false);

  const tokenById = useMemo(() => new Map(tokens.map((token) => [token.id, token])), [tokens]);

  const loadSwaps = useCallback(async () => {
    if (!address) return;

    setLoading(true);
    setUnavailable(false);
    try {
      setSwaps(await fetchUserSwaps(address));
    } catch {
      setUnavailable(true);
    } finally {
      setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    if (!address) {
      setSwaps([]);
      setUnavailable(false);
      setLoading(false);
      return;
    }
    void loadSwaps();
  }, [address, loadSwaps]);

  useEffect(() => {
    if (!address) return;

    let timeout: ReturnType<typeof setTimeout> | undefined;
    const refetchAfterIndexing = () => {
      timeout = setTimeout(() => void loadSwaps(), 2_000);
    };

    window.addEventListener(SWAP_SUCCESS_EVENT, refetchAfterIndexing);
    return () => {
      window.removeEventListener(SWAP_SUCCESS_EVENT, refetchAfterIndexing);
      if (timeout) clearTimeout(timeout);
    };
  }, [address, loadSwaps]);

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
    return (
      <section className="surface-panel px-4 py-3">
        <h2 className="text-[15px] font-semibold text-zinc-100">Swap history</h2>
        <div className="mt-2 flex items-center justify-between gap-3">
          <p className="text-[12px] text-zinc-500">Connect wallet to see your swaps</p>
          <button
            type="button"
            onClick={connect}
            disabled={connecting}
            className="btn-primary shrink-0 px-3 py-1.5 text-[12px]"
          >
            {connecting ? 'Connecting...' : 'Connect'}
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="surface-panel overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 border-b border-white/[0.06]">
        <h2 className="text-[15px] font-semibold text-zinc-100">Swap history</h2>
        {loading && <span className="text-[11px] text-zinc-500">Loading…</span>}
      </div>

      {unavailable ? (
        <p className="px-4 py-3 text-[12px] text-zinc-500">History unavailable</p>
      ) : swaps.length === 0 && !loading ? (
        <p className="px-4 py-3 text-[12px] text-zinc-500">No swaps yet</p>
      ) : (
        <div className="divide-y divide-white/[0.06]">
          {swaps.map((swap) => (
            <a
              key={`${swap.tx_hash}-${swap.ledger}`}
              href={`https://stellar.expert/explorer/public/tx/${swap.tx_hash}`}
              target="_blank"
              rel="noopener noreferrer"
              className="block px-4 py-3 hover:bg-white/[0.03] transition-colors"
            >
              <div className="flex items-center justify-between gap-3">
                <span className="text-[12px] text-zinc-500">{relativeTime(swap.created_at)}</span>
                <span className={swap.status === 'SUCCESS' ? 'text-[11px] text-emerald-400' : 'text-[11px] text-zinc-500'}>
                  {swap.status}
                </span>
              </div>
              <div className="mt-1 flex items-center gap-1.5 text-[13px] tabular-nums">
                <span className="text-zinc-200">
                  {amountLabel(swap.amount_in, swap.token_in)} {tokenLabel(swap.token_in)}
                </span>
                <span className="text-zinc-600">→</span>
                <span className="text-zinc-100">
                  {amountLabel(swap.amount_out, swap.token_out)} {tokenLabel(swap.token_out)}
                </span>
              </div>
            </a>
          ))}
        </div>
      )}
    </section>
  );
}

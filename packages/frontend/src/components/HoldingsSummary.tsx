'use client';

import Link from 'next/link';
import { useEffect, useMemo, useState } from 'react';
import { formatBalanceDisplay } from '@/lib/balance';
import { fetchPrices, type Price } from '@/lib/prices';
import { useAccountBalances } from '@/lib/account-balances-context';
import { useWallet } from '@/lib/wallet-context';
import { useTokenList } from './TokenSelector';

function shortContractId(contractId: string): string {
  return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
}

function formatUsd(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return value.toLocaleString(undefined, {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: value >= 1 ? 2 : 4,
  });
}

export function HoldingsSummary() {
  const { address, connect, connecting } = useWallet();
  const { balances, ready, loading } = useAccountBalances();
  const tokens = useTokenList();
  const [prices, setPrices] = useState<Map<string, Price>>(new Map());

  const tokenById = useMemo(() => new Map(tokens.map((token) => [token.id, token])), [tokens]);
  const holdings = useMemo(
    () =>
      Object.entries(balances)
        .filter(([, balance]) => balance > BigInt(0))
        .map(([id, balance]) => {
          const token = tokenById.get(id);
          return {
            id,
            balance,
            decimals: token?.decimals ?? 7,
            symbol: token?.symbol ?? shortContractId(id),
          };
        }),
    [balances, tokenById],
  );

  const holdingIds = useMemo(() => holdings.map((holding) => holding.id), [holdings]);
  const holdingKey = holdingIds.join(',');

  useEffect(() => {
    let cancelled = false;
    if (!address || holdingIds.length === 0) {
      setPrices(new Map());
      return;
    }

    void fetchPrices(holdingIds)
      .then((nextPrices) => {
        if (!cancelled) setPrices(nextPrices);
      })
      .catch(() => {
        if (!cancelled) setPrices(new Map());
      });

    return () => {
      cancelled = true;
    };
  }, [address, holdingKey, holdingIds]);

  const valuedHoldings = useMemo(
    () =>
      holdings
        .map((holding) => {
          const price = prices.get(holding.id)?.price_usdc;
          const amount = Number(holding.balance) / 10 ** holding.decimals;
          return {
            ...holding,
            amount,
            value: price === undefined ? null : amount * price,
          };
        })
        .sort((a, b) => (b.value ?? -1) - (a.value ?? -1)),
    [holdings, prices],
  );

  const total = valuedHoldings.reduce<number | null>(
    (sum, holding) => (holding.value === null || sum === null ? null : sum + holding.value),
    0,
  );

  if (!address) {
    return (
      <section className="surface-panel px-4 py-3">
        <h2 className="text-[15px] font-semibold text-zinc-100">Holdings</h2>
        <div className="mt-2 flex items-center justify-between gap-3">
          <p className="text-[12px] text-zinc-500">Connect wallet to see your holdings</p>
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
        <div>
          <h2 className="text-[15px] font-semibold text-zinc-100">Holdings</h2>
          <p className="mt-0.5 text-[12px] text-zinc-500">
            Total value <span className="tabular-nums text-zinc-300">{formatUsd(total)}</span>
          </p>
        </div>
        {loading && <span className="text-[11px] text-zinc-500">Loading…</span>}
      </div>

      {!ready && !loading ? (
        <p className="px-4 py-3 text-[12px] text-zinc-500">Holdings unavailable</p>
      ) : valuedHoldings.length === 0 && !loading ? (
        <p className="px-4 py-3 text-[12px] text-zinc-500">No token balances yet</p>
      ) : (
        <div className="divide-y divide-white/[0.06]">
          {valuedHoldings.slice(0, 5).map((holding) => (
            <div key={holding.id} className="flex items-center justify-between gap-3 px-4 py-2.5">
              <div className="min-w-0">
                <p className="truncate text-[13px] text-zinc-200">{holding.symbol}</p>
                <p className="truncate text-[11px] tabular-nums text-zinc-500">
                  {formatBalanceDisplay(holding.balance, holding.decimals)}
                </p>
              </div>
              <p className="shrink-0 text-[13px] tabular-nums text-zinc-300">
                {formatUsd(holding.value)}
              </p>
            </div>
          ))}
        </div>
      )}

      <Link
        href="/portfolio"
        className="block border-t border-white/[0.06] px-4 py-2.5 text-[12px] font-medium text-emerald-400 hover:bg-white/[0.03] transition-colors"
      >
        View portfolio →
      </Link>
    </section>
  );
}

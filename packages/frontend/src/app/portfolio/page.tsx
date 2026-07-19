'use client';

import { useEffect, useMemo, useState } from 'react';
import { Sparkline } from '@/components/Sparkline';
import { useTokenList } from '@/components/TokenSelector';
import { formatBalanceDisplay } from '@/lib/balance';
import { useAccountBalances } from '@/lib/account-balances-context';
import { fetchPriceHistory, fetchPrices, type Price, type PriceHistoryPoint } from '@/lib/prices';
import { useWallet } from '@/lib/wallet-context';

const HISTORY_CONCURRENCY = 5;

interface Holding {
  id: string;
  balance: bigint;
  decimals: number;
  symbol: string;
}

function shortContractId(contractId: string): string {
  return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
}

function formatUsd(value: number | null, maximumFractionDigits = 2): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return value.toLocaleString(undefined, {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: value >= 1 ? maximumFractionDigits : 6,
  });
}

async function fetchHistories(ids: string[]): Promise<Map<string, PriceHistoryPoint[]>> {
  const histories = new Map<string, PriceHistoryPoint[]>();
  let nextIndex = 0;

  async function worker() {
    while (nextIndex < ids.length) {
      const id = ids[nextIndex++];
      try {
        histories.set(id, await fetchPriceHistory(id));
      } catch {
        histories.set(id, []);
      }
    }
  }

  await Promise.all(Array.from({ length: Math.min(HISTORY_CONCURRENCY, ids.length) }, worker));
  return histories;
}

export default function PortfolioPage() {
  const { address, connect, connecting } = useWallet();
  const { balances, ready, loading } = useAccountBalances();
  const tokens = useTokenList();
  const [prices, setPrices] = useState<Map<string, Price>>(new Map());
  const [histories, setHistories] = useState<Map<string, PriceHistoryPoint[]>>(new Map());
  const [pricingLoading, setPricingLoading] = useState(false);

  const tokenById = useMemo(() => new Map(tokens.map((token) => [token.id, token])), [tokens]);
  const holdings = useMemo<Holding[]>(
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
      setHistories(new Map());
      setPricingLoading(false);
      return;
    }

    setPricingLoading(true);
    setPrices(new Map());
    setHistories(new Map());

    void Promise.all([
      fetchPrices(holdingIds).catch(() => new Map<string, Price>()),
      fetchHistories(holdingIds),
    ]).then(([nextPrices, nextHistories]) => {
      if (!cancelled) {
        setPrices(nextPrices);
        setHistories(nextHistories);
        setPricingLoading(false);
      }
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
            price: price ?? null,
            value: price === undefined ? null : amount * price,
            history: histories.get(holding.id) ?? [],
          };
        })
        .sort((a, b) => (b.value ?? -1) - (a.value ?? -1)),
    [histories, holdings, prices],
  );
  const total = valuedHoldings.reduce<number | null>(
    (sum, holding) => (holding.value === null || sum === null ? null : sum + holding.value),
    0,
  );

  if (!address) {
    return (
      <section className="min-h-[420px] flex flex-col items-center justify-center text-center">
        <div className="w-full max-w-md surface-panel px-6 py-8">
          <div className="mx-auto mb-4 flex h-11 w-11 items-center justify-center rounded-xl border border-emerald-400/15 bg-emerald-400/[0.07] text-emerald-300">
            $
          </div>
          <h1 className="text-xl font-semibold tracking-tight text-zinc-50">Your portfolio</h1>
          <p className="mx-auto mt-2 max-w-sm text-[13px] leading-relaxed text-zinc-500">
            Connect your Stellar wallet to see token balances, USD values, and 24-hour price trends.
          </p>
          <button
            type="button"
            onClick={connect}
            disabled={connecting}
            className="btn-primary mt-6 px-5 py-2.5 text-sm disabled:opacity-50"
          >
            {connecting ? 'Connecting...' : 'Connect wallet'}
          </button>
        </div>
      </section>
    );
  }

  return (
    <div className="w-full space-y-7 pb-12">
      <section className="flex flex-col gap-3 border-b border-white/[0.06] pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="section-label">Wallet</p>
          <h1 className="mt-2 text-2xl font-semibold tracking-tight text-zinc-50 sm:text-3xl">Portfolio</h1>
          <p className="mt-2 text-[13px] text-zinc-500">Current wallet balances valued through LumAgg quotes.</p>
        </div>
        <div className="rounded-xl border border-emerald-400/15 bg-emerald-400/[0.05] px-4 py-3 sm:min-w-52">
          <div className="text-[11px] font-medium uppercase tracking-wide text-emerald-300/80">Total value</div>
          <div className="mt-1 text-2xl font-semibold tracking-tight tabular-nums text-zinc-50">{formatUsd(total)}</div>
        </div>
      </section>

      {!ready && !loading ? (
        <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.04] px-4 py-3 text-sm text-amber-200/90">
          Holdings are unavailable right now. Try reconnecting your wallet.
        </div>
      ) : loading ? (
        <div className="grid gap-3">
          {Array.from({ length: 4 }).map((_, index) => (
            <div key={index} className="h-16 animate-pulse rounded-xl border border-white/[0.06] bg-zinc-900/40" />
          ))}
        </div>
      ) : valuedHoldings.length === 0 ? (
        <div className="surface-panel px-5 py-10 text-center">
          <h2 className="text-[15px] font-medium text-zinc-200">No token balances yet</h2>
          <p className="mt-1 text-[13px] text-zinc-500">Your non-zero Stellar token balances will appear here.</p>
        </div>
      ) : (
        <section className="overflow-hidden rounded-xl border border-white/[0.08] bg-zinc-900/35">
          <div className="flex items-center justify-between border-b border-white/[0.06] px-4 py-3 sm:px-5">
            <div>
              <h2 className="text-[15px] font-medium text-zinc-100">Holdings</h2>
              <p className="mt-0.5 text-[12px] text-zinc-500">24-hour charts use price ticks sampled by LumAgg.</p>
            </div>
            {pricingLoading && <span className="text-[11px] text-zinc-500">Loading prices…</span>}
          </div>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[680px] text-left text-[13px]">
              <thead className="bg-zinc-900/60 text-[11px] uppercase tracking-wide text-zinc-500">
                <tr>
                  <th className="px-4 py-3 font-medium sm:px-5">Token</th>
                  <th className="px-3 py-3 text-right font-medium">Balance</th>
                  <th className="px-3 py-3 text-right font-medium">Price</th>
                  <th className="px-3 py-3 text-right font-medium">Value</th>
                  <th className="px-4 py-3 text-right font-medium sm:px-5">24h</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/[0.06]">
                {valuedHoldings.map((holding) => (
                  <tr key={holding.id} className="transition-colors hover:bg-white/[0.025]">
                    <td className="px-4 py-3 sm:px-5">
                      <div className="font-medium text-zinc-200">{holding.symbol}</div>
                      <div className="mt-0.5 max-w-36 truncate font-mono text-[11px] text-zinc-600">{holding.id}</div>
                    </td>
                    <td className="px-3 py-3 text-right tabular-nums text-zinc-300">
                      {formatBalanceDisplay(holding.balance, holding.decimals)}
                    </td>
                    <td className="px-3 py-3 text-right tabular-nums text-zinc-400">{formatUsd(holding.price, 6)}</td>
                    <td className="px-3 py-3 text-right font-medium tabular-nums text-zinc-200">{formatUsd(holding.value)}</td>
                    <td className="px-4 py-3 sm:px-5">
                      <div className="flex justify-end">
                        <Sparkline points={holding.history} />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}
    </div>
  );
}

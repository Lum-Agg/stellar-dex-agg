'use client';

import { useEffect, useMemo, useState } from 'react';
import { useTokenList } from '@/components/TokenSelector';
import { HoldingsTable, type ValuedHolding } from '@/components/portfolio/HoldingsTable';
import { ProfileHero } from '@/components/portfolio/ProfileHero';
import { ProfileTabs, type ProfileTab } from '@/components/portfolio/ProfileTabs';
import { SwapHistory } from '@/components/SwapHistory';
import { useAccountBalances } from '@/lib/account-balances-context';
import { fetchPriceHistory, fetchPrices, type Price, type PriceHistoryPoint } from '@/lib/prices';
import { useWallet } from '@/lib/wallet-context';

const HISTORY_CONCURRENCY = 5;

function shortContractId(contractId: string): string {
  return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
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
  const [activeTab, setActiveTab] = useState<ProfileTab>('holdings');
  const [prices, setPrices] = useState<Map<string, Price>>(new Map());
  const [histories, setHistories] = useState<Map<string, PriceHistoryPoint[]>>(new Map());
  const [pricingLoading, setPricingLoading] = useState(false);

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
            token,
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

  const valuedHoldings = useMemo<ValuedHolding[]>(
    () =>
      holdings
        .map((holding) => {
          const price = prices.get(holding.id)?.price_usdc;
          const amount = Number(holding.balance) / 10 ** holding.decimals;
          return {
            id: holding.id,
            balance: holding.balance,
            decimals: holding.decimals,
            symbol: holding.symbol,
            token: holding.token,
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
      <section className="flex flex-col items-center pt-2 md:pt-6 text-center">
        <div className="w-full max-w-md surface-panel px-6 py-8">
          <div className="mx-auto mb-4 flex h-11 w-11 items-center justify-center rounded-xl border border-[var(--accent)]/20 bg-[var(--accent)]/10 text-[var(--accent)]">
            $
          </div>
          <h1 className="text-xl font-semibold tracking-tight text-[var(--text-primary)]">
            Your portfolio
          </h1>
          <p className="mx-auto mt-2 max-w-sm text-[13px] leading-relaxed text-[var(--text-muted)]">
            Connect your wallet to view holdings and LumAgg swap history.
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
    <div className="mx-auto w-full max-w-5xl space-y-6 pb-12">
      <ProfileHero address={address} total={total} pricingLoading={pricingLoading} />

      <ProfileTabs
        active={activeTab}
        onChange={setActiveTab}
        trailing={activeTab === 'holdings' && pricingLoading ? 'Updating prices…' : undefined}
      />

      <div className="pt-2">
        {activeTab === 'holdings' && (
          <>
            {!ready && !loading ? (
              <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.04] px-4 py-3 text-[14px] text-amber-200/90">
                Holdings are unavailable right now. Try reconnecting your wallet.
              </div>
            ) : loading ? (
              <div className="grid gap-3">
                {Array.from({ length: 4 }).map((_, index) => (
                  <div
                    key={index}
                    className="h-16 animate-pulse rounded-xl border border-[var(--border)] bg-[var(--surface)]/40"
                  />
                ))}
              </div>
            ) : (
              <HoldingsTable holdings={valuedHoldings} />
            )}
          </>
        )}

        {activeTab === 'history' && <SwapHistory variant="profile" />}
      </div>
    </div>
  );
}

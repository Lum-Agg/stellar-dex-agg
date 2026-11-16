'use client';

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import Link from 'next/link';
import { displayTokenSymbol } from '@/lib/tokenDisplay';
import { fetchJson } from '@/lib/fetch-json';
import { StatsCharts } from '@/components/stats/StatsCharts';
import { activitySplit } from '@/lib/stats-activity';
import { selectChartPeriod } from '@/lib/stats-chart-data';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';
const AUTO_REFRESH_MS = 2 * 60 * 1000;

interface TokenVolume {
  token: string;
  amount_in: string | number;
  routed_volume: string | number;
  routed_leg_count?: number;
}

interface RoutedTokenVolume {
  token: string;
  routed_volume: string | number;
  routed_leg_count: number;
}

interface RoundTripSurplus {
  base_token: string;
  tx_count: number;
  amount_in: string | number;
  gross_surplus: string | number;
  gross_surplus_usd?: number | null;
}

interface RoundTripBridgeStats {
  bridge_token: string;
  tx_count: number;
  amount_in: string | number;
  gross_surplus: string | number;
}

interface DailyStats {
  day: string;
  tx_count: number;
  unique_users: number;
  /** Sum of entry amounts — mixed tokens; do not treat as XLM. */
  total_amount_in: string | number;
  /** Actual DEX leg inputs — mixed token units; prefer USD fields. */
  total_routed_dex_volume?: string | number;
  routed_leg_count?: number;
  routed_priced_leg_count?: number;
  routed_pricing_coverage?: number | null;
  by_token?: TokenVolume[];
  routed_by_token?: RoutedTokenVolume[];
  by_function: Record<string, number>;
  by_dex: Record<string, number>;
  round_trip_count?: number;
  round_trip_by_token?: RoundTripSurplus[];
  round_trip_by_bridge?: RoundTripBridgeStats[];
  round_trip_gross_surplus_usd?: number | null;
  split_swap_count: number;
  success_count: number;
  failed_count: number;
  xlm_usd?: number | null;
  total_amount_in_usd?: number | null;
  total_routed_dex_volume_usd?: number | null;
}

function dayRoutedUsd(d: DailyStats): number | null {
  if (
    typeof d.total_routed_dex_volume_usd === 'number' &&
    Number.isFinite(d.total_routed_dex_volume_usd)
  ) {
    return d.total_routed_dex_volume_usd;
  }
  return null;
}

function dayNotionalUsd(d: DailyStats): number | null {
  if (typeof d.total_amount_in_usd === 'number' && Number.isFinite(d.total_amount_in_usd)) {
    return d.total_amount_in_usd;
  }
  return null;
}

function dayGrossSurplusUsd(d: DailyStats): number | null {
  if (
    typeof d.round_trip_gross_surplus_usd === 'number' &&
    Number.isFinite(d.round_trip_gross_surplus_usd)
  ) {
    return d.round_trip_gross_surplus_usd;
  }
  return null;
}

interface StatsPayload {
  db_path: string;
  invocation_count: number;
  cursor_ledger: number | null;
  oldest_created_at: number | null;
  daily: DailyStats[];
  usd_pricing?: string | null;
}

interface ApiResponse<T> {
  success: boolean;
  data: T;
  error?: string;
}

const DEX_COLORS: Record<string, string> = {
  aquarius: '#3dd6c6',
  aquarius_clmm: '#2fc4b4',
  phoenix: '#f59e0b',
  soroswap: '#22d3ee',
  sushi: '#34d399',
  comet: '#fb7185',
};

function dexColor(name: string, index: number): string {
  if (DEX_COLORS[name]) return DEX_COLORS[name];
  const fallback = ['#3dd6c6', '#2dd4bf', '#fbbf24', '#f472b6', '#94a3b8'];
  return fallback[index % fallback.length];
}

function formatUsd(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n >= 1_000_000) return `$${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `$${(n / 1_000).toFixed(2)}K`;
  return `$${n.toLocaleString(undefined, { maximumFractionDigits: 2 })}`;
}

type ChartRange = 14 | 30 | 90 | 'all';

const CHART_RANGES: { id: ChartRange; label: string }[] = [
  { id: 14, label: '14d' },
  { id: 30, label: '30d' },
  { id: 90, label: '90d' },
  { id: 'all', label: 'All' },
];

export default function StatsPage() {
  const [data, setData] = useState<StatsPayload | null>(null);
  const [tokenLabels, setTokenLabels] = useState<Map<string, string>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [chartRange, setChartRange] = useState<ChartRange>(30);
  const [refreshKey, setRefreshKey] = useState(0);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const statsRes = await fetchJson<ApiResponse<StatsPayload>>(`${API_URL}/api/v1/stats`, {
          cache: 'no-store',
        });
        if (!statsRes.success) {
          throw new Error(statsRes.error || 'stats request failed');
        }
        if (!cancelled) {
          setData(statsRes.data);
          setError(null);
          setLastUpdated(new Date());
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshKey]);

  useEffect(() => {
    const refreshWhenVisible = () => {
      if (document.visibilityState === 'visible') {
        setRefreshKey((value) => value + 1);
      }
    };
    const interval = window.setInterval(refreshWhenVisible, AUTO_REFRESH_MS);
    document.addEventListener('visibilitychange', refreshWhenVisible);
    return () => {
      window.clearInterval(interval);
      document.removeEventListener('visibilitychange', refreshWhenVisible);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    fetchJson<{ tokens?: Array<{ id?: string; symbol?: string }> }>(`${API_URL}/api/v1/tokens`, {
      cache: 'no-store',
    })
      .then((payload) => {
        if (cancelled || !Array.isArray(payload.tokens)) return;
        const labels = new Map<string, string>();
        for (const token of payload.tokens) {
          if (!token || typeof token.id !== 'string' || typeof token.symbol !== 'string') continue;
          labels.set(token.id, displayTokenSymbol(token.symbol, token.id));
        }
        setTokenLabels(labels);
      })
      .catch(() => {
        // Address fallback remains useful when token metadata is unavailable.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const derived = useMemo(() => {
    if (!data) return null;
    const days = [...data.daily].sort((a, b) => a.day.localeCompare(b.day));
    let notionalUsd = 0;
    let routedUsd = 0;
    let usdCovered = 0;
    let routedUsdCovered = 0;
    let txs = 0;
    let roundTrips = 0;
    let grossSurplusUsd = 0;
    let surplusPricedDays = 0;
    let surplusPricedRoundTrips = 0;
    let routedLegs = 0;
    let routedPricedLegs = 0;
    const dexTotals: Record<string, number> = {};
    const fnTotals: Record<string, number> = {};
    const bridgeTotals: Record<string, number> = {};

    for (const d of days) {
      txs += d.tx_count;
      roundTrips += d.round_trip_count ?? 0;
      routedLegs += Object.values(d.by_dex).reduce((sum, count) => sum + count, 0);
      routedPricedLegs += d.routed_priced_leg_count ?? 0;
      const nUsd = dayNotionalUsd(d);
      const rUsd = dayRoutedUsd(d);
      const surplusUsd = dayGrossSurplusUsd(d);
      if (nUsd != null) {
        notionalUsd += nUsd;
        usdCovered += 1;
      }
      if (rUsd != null) {
        routedUsd += rUsd;
        routedUsdCovered += 1;
      }
      if (surplusUsd != null) {
        grossSurplusUsd += surplusUsd;
        surplusPricedDays += 1;
        surplusPricedRoundTrips += (d.round_trip_by_token ?? [])
          .filter((row) => typeof row.gross_surplus_usd === 'number')
          .reduce((sum, row) => sum + row.tx_count, 0);
      }
      for (const [k, v] of Object.entries(d.by_dex)) {
        dexTotals[k] = (dexTotals[k] || 0) + v;
      }
      for (const [k, v] of Object.entries(d.by_function)) {
        fnTotals[k] = (fnTotals[k] || 0) + v;
      }
      for (const row of d.round_trip_by_bridge ?? []) {
        if (!row.bridge_token) continue;
        bridgeTotals[row.bridge_token] = (bridgeTotals[row.bridge_token] || 0) + row.tx_count;
      }
    }

    const venuesHit = Object.keys(dexTotals).length;
    const pricedDays = days.filter((d) => typeof d.xlm_usd === 'number');
    const avgXlmUsd =
      pricedDays.length > 0
        ? pricedDays.reduce((s, d) => s + (d.xlm_usd as number), 0) / pricedDays.length
        : null;

    return {
      days,
      notionalUsd: usdCovered > 0 ? notionalUsd : null,
      routedUsd: routedUsdCovered > 0 ? routedUsd : null,
      avgXlmUsd,
      txs,
      activity: activitySplit(fnTotals, txs),
      routedPricingCoverage:
        routedLegs > 0 ? Math.min(1, Math.max(0, routedPricedLegs / routedLegs)) : null,
      roundTrips,
      grossSurplusUsd: surplusPricedDays > 0 ? grossSurplusUsd : null,
      averageGrossSurplusUsd:
        surplusPricedRoundTrips > 0 ? grossSurplusUsd / surplusPricedRoundTrips : null,
      surplusPricedDays,
      venuesHit,
      dexTotals,
      fnTotals,
      bridgeTotals,
    };
  }, [data]);

  const chartDays = useMemo(() => {
    if (!derived) return [];
    return selectChartPeriod(derived.days, chartRange);
  }, [derived, chartRange]);

  return (
    <div className="max-w-5xl mx-auto space-y-8">
      <div className="flex flex-col sm:flex-row sm:items-end sm:justify-between gap-3">
        <div>
          <h1 className="text-2xl sm:text-3xl font-semibold tracking-tight text-[var(--text-primary)]">
            LumAgg on-chain activity
          </h1>
          <p className="text-[13px] text-[var(--text-secondary)] mt-2 leading-relaxed max-w-xl">
            Aggregator contract invocations only — volume LumAgg routed onto Stellar Soroban DEXes,
            not market-wide pool volume. Routed volume sums the actual input value processed by
            every executed DEX leg. Split routes use each pool&apos;s real allocation; multi-hop and
            round-trip legs are priced in their own input token (XLM = historical daily reference;
            USDC = $1). Only priced actual amounts are included; missing data is not estimated.
          </p>
        </div>
        {data && (
          <div className="flex self-start flex-col items-start gap-2 sm:self-auto sm:items-end">
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => {
                  setError(null);
                  setLoading(true);
                  setRefreshKey((value) => value + 1);
                }}
                disabled={loading}
                className="text-[12px] px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--surface)]/80 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] disabled:cursor-wait disabled:opacity-50 transition-colors"
              >
                {loading ? 'Refreshing…' : 'Refresh'}
              </button>
              <a
                href="https://defillama.com/dex-aggregators/chain/stellar"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[12px] px-3 py-1.5 rounded-lg border border-teal-400/25 bg-teal-400/5 text-teal-200 hover:border-teal-300/50 hover:bg-teal-400/10 transition-colors"
              >
                View on DefiLlama
              </a>
              <a
                href={`${API_URL}/api/v1/stats?format=csv`}
                className="text-[12px] px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--surface)]/80 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] transition-colors"
              >
                Export CSV
              </a>
            </div>
            {lastUpdated && (
              <span className="text-[11px] text-[var(--text-muted)] tabular-nums">
                Updated{' '}
                {lastUpdated.toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
                  second: '2-digit',
                })}
                {' · '}auto-refreshes every 2 min
              </span>
            )}
          </div>
        )}
      </div>

      {loading && !data && (
        <div role="status" className="space-y-3">
          <p className="text-[12px] text-[var(--text-muted)]">Loading on-chain analytics…</p>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div
                key={i}
                className="h-[88px] rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 animate-pulse"
              />
            ))}
          </div>
        </div>
      )}

      {error && (
        <div
          role="alert"
          className="flex flex-col gap-3 rounded-xl border border-amber-500/20 bg-amber-500/5 px-4 py-3 text-sm text-amber-300/90 sm:flex-row sm:items-center sm:justify-between"
        >
          <span>
            {data
              ? `Could not refresh stats; showing the last loaded snapshot: ${error}`
              : `Stats are temporarily unavailable: ${error}`}
          </span>
          <button
            type="button"
            onClick={() => {
              setError(null);
              setLoading(true);
              setRefreshKey((value) => value + 1);
            }}
            className="self-start whitespace-nowrap rounded-lg border border-amber-300/20 px-3 py-1.5 text-[12px] font-medium text-amber-100 hover:border-amber-300/40 hover:bg-amber-300/[0.06] sm:self-auto"
          >
            Try again
          </button>
        </div>
      )}

      {data && derived && (
        <>
          <section aria-label="User swap and arbitrage breakdown" className="space-y-3">
            <h2 className="text-lg font-medium text-[var(--text-primary)]">
              User swaps vs arbitrage
            </h2>
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
              <KpiCard
                label="User swaps"
                value={derived.activity.swaps.toLocaleString()}
                hint={
                  derived.activity.swapShare == null
                    ? 'No successful transactions'
                    : `${derived.activity.swapShare.toFixed(2)}% of successful transactions`
                }
                accent
              />
              <KpiCard
                label="Arbitrage transactions"
                value={derived.activity.arbitrage.toLocaleString()}
                hint={
                  derived.activity.arbitrageShare == null
                    ? 'No successful transactions'
                    : `${derived.activity.arbitrageShare.toFixed(2)}% of successful transactions`
                }
              />
              <KpiCard
                label="Other / unclassified"
                value={derived.activity.other.toLocaleString()}
                hint="Not counted as user swaps"
              />
              <KpiCard
                label="All successful transactions"
                value={derived.txs.toLocaleString()}
                hint="User swaps + arbitrage + other"
              />
            </div>
            <p className="text-[12px] text-[var(--text-secondary)]">
              Full indexed period. Classified by successful contract calls: swap = user swap;
              round_trip_swap = arbitrage. User swaps may include bots or team activity, not
              necessarily independent users. Arbitrage is not user adoption. All volume metrics
              below combine user swaps and arbitrage; they are not user-only trading volume.
            </p>
          </section>
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <KpiCard
              label="Combined routed (priced USD)"
              value={derived.routedUsd != null ? formatUsd(derived.routedUsd) : '—'}
              hint={
                derived.routedPricingCoverage != null
                  ? `Includes arbitrage · ${(derived.routedPricingCoverage * 100).toFixed(0)}% legs priced`
                  : 'Includes arbitrage · actual DEX leg inputs'
              }
              accent
              delay={0}
            />
            <KpiCard
              label="Combined notional (priced USD)"
              value={derived.notionalUsd != null ? formatUsd(derived.notionalUsd) : '—'}
              hint={
                derived.avgXlmUsd != null
                  ? `XLM day avg ≈ $${derived.avgXlmUsd.toFixed(4)}`
                  : 'priced inputs only'
              }
              delay={60}
            />
            <KpiCard
              label="All successful transactions"
              value={derived.txs.toLocaleString()}
              hint={`${derived.venuesHit} venues · ${data.invocation_count.toLocaleString()} indexed`}
              delay={120}
            />
            <KpiCard
              label="Days priced"
              value={String(data.daily.filter((d) => dayNotionalUsd(d) != null).length)}
              hint={data.usd_pricing ? 'notional days with USD' : 'USD enrichment offline'}
              delay={180}
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="text-lg font-medium tracking-tight text-[var(--text-primary)]">
                Activity explorer
              </h2>
              <p className="mt-1 text-xs text-[var(--text-muted)]" aria-live="polite">
                {chartDays.length} indexed days
                {chartDays.length
                  ? ` · ${chartDays[0].day} – ${chartDays[chartDays.length - 1].day}`
                  : ''}
                . All charts use the same UTC period.
              </p>
            </div>
            <div
              className="inline-flex rounded-xl border border-[var(--border)] bg-[var(--surface)] p-1"
              role="group"
              aria-label="Chart date range"
            >
              {CHART_RANGES.map(({ id, label }) => (
                <button
                  key={String(id)}
                  type="button"
                  onClick={() => setChartRange(id)}
                  aria-pressed={chartRange === id}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${chartRange === id ? 'bg-[var(--accent)] text-[var(--accent-contrast)]' : 'text-[var(--text-secondary)] hover:bg-[var(--surface-raised)]'}`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <StatsCharts days={chartDays} />

          <section className="rounded-xl border border-teal-400/20 bg-[linear-gradient(115deg,rgba(45,212,191,0.08),rgba(15,23,42,0.18))] px-4 sm:px-5 py-4">
            <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2">
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text-primary)]">
                  Arbitrage execution
                </h2>
                <p className="text-[12px] text-[var(--text-muted)] mt-1 max-w-2xl">
                  Successful on-chain round trips. Gross surplus is the actual base token returned
                  minus the base token supplied; it excludes transaction fees and is not net
                  P&amp;L. See the{' '}
                  <Link
                    href="/arbitrage"
                    className="text-teal-300/90 hover:text-teal-200 underline underline-offset-2"
                  >
                    Arbitrage
                  </Link>{' '}
                  page for recent trades.
                </p>
              </div>
              <span className="text-[10px] uppercase tracking-[0.16em] text-teal-300/70 whitespace-nowrap">
                On-chain actuals
              </span>
            </div>
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-x-5 gap-y-4 mt-5">
              <ArbMetric
                label="Gross surplus (USD)"
                value={derived.grossSurplusUsd != null ? formatUsd(derived.grossSurplusUsd) : '—'}
              />
              <ArbMetric
                label="Successful round trips"
                value={derived.roundTrips.toLocaleString()}
              />
              <ArbMetric
                label="Avg gross / round trip"
                value={
                  derived.averageGrossSurplusUsd != null
                    ? formatUsd(derived.averageGrossSurplusUsd)
                    : '—'
                }
              />
              <ArbMetric
                label="Surplus days priced"
                value={`${derived.surplusPricedDays} / ${derived.days.length}`}
              />
            </div>
          </section>

          <div className="grid lg:grid-cols-3 gap-4">
            <SharePanel
              title="DEX legs"
              subtitle="Hop counts via LumAgg (not full-market volume)"
              entries={Object.entries(derived.dexTotals).sort((a, b) => b[1] - a[1])}
              colorFn={dexColor}
            />
            <SharePanel
              title="By function"
              subtitle="Contract entrypoints"
              entries={Object.entries(derived.fnTotals).sort((a, b) => b[1] - a[1])}
              colorFn={(name, i) =>
                name.includes('round_trip')
                  ? '#3dd6c6'
                  : name.includes('split')
                    ? '#22d3ee'
                    : dexColor(name, i)
              }
            />
            <SharePanel
              title="Arbitrage bridges"
              subtitle="Successful round trips by intermediary token"
              entries={Object.entries(derived.bridgeTotals).sort((a, b) => b[1] - a[1])}
              colorFn={() => '#f59e0b'}
              displayName={(address) => tokenLabels.get(address) ?? shortTokenAddress(address)}
            />
          </div>

          <div className="flex flex-wrap gap-x-5 gap-y-2 rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/40 px-4 py-3 text-[12px] text-[var(--text-muted)]">
            <OpsItem label="Cursor ledger" value={data.cursor_ledger?.toLocaleString() ?? '—'} />
            <OpsItem label="Days indexed" value={String(data.daily.length)} />
            <OpsItem label="USD pricing" value={data.usd_pricing ? 'per-token day close' : '—'} />
            <OpsItem
              label="API"
              value={
                <a
                  href={`${API_URL}/api/v1/stats`}
                  className="text-[var(--text-secondary)] hover:text-[var(--text-primary)] underline underline-offset-2"
                  target="_blank"
                  rel="noreferrer"
                >
                  /api/v1/stats
                </a>
              }
            />
          </div>

          <details open className="group rounded-xl border border-[var(--border)] bg-[var(--surface)]/50 open:bg-[var(--surface)]/60">
            <summary className="cursor-pointer list-none px-4 sm:px-5 py-3.5 flex items-center justify-between text-[14px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] [&::-webkit-details-marker]:hidden">
              <span className="font-medium">Daily rollup</span>
              <span className="text-[12px] text-[var(--text-muted)]">
                <span className="group-open:hidden">Show table</span>
                <span className="hidden group-open:inline">Hide</span>
              </span>
            </summary>
            <div
              role="region"
              aria-label="Daily rollup table"
              tabIndex={0}
              className="max-h-[min(60vh,28rem)] overflow-auto rounded-b-xl border-t border-[var(--border)]"
            >
              <table className="w-full text-[12px] text-left">
                <thead className="sticky top-0 z-10 bg-[var(--surface)] text-[var(--text-muted)]">
                  <tr>
                    <th className="px-3 py-2 font-medium">Day</th>
                    <th className="px-3 py-2 font-medium">Transactions</th>
                    <th className="px-3 py-2 font-medium">User swaps</th>
                    <th className="px-3 py-2 font-medium">Arbitrage</th>
                    <th className="px-3 py-2 font-medium">All caller addresses</th>
                    <th className="px-3 py-2 font-medium">Notional USD</th>
                    <th className="px-3 py-2 font-medium">Routed USD</th>
                    <th className="px-3 py-2 font-medium">RT gross USD</th>
                    <th className="px-3 py-2 font-medium">XLM/USD</th>
                    <th className="px-3 py-2 font-medium">Split</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--border)]">
                  {[...derived.days].reverse().map((d) => {
                    const { swaps, arbitrage } = activitySplit(d.by_function, d.tx_count);
                    const notionalUsd = dayNotionalUsd(d);
                    const routedUsd = dayRoutedUsd(d);
                    const grossSurplusUsd = dayGrossSurplusUsd(d);
                    return (
                      <tr key={d.day} className="text-[var(--text-secondary)]">
                        <td className="px-3 py-2 whitespace-nowrap">{d.day}</td>
                        <td className="px-3 py-2">{d.tx_count}</td>
                        <td className="px-3 py-2">
                          {swaps}
                        </td>
                        <td className="px-3 py-2">
                          {arbitrage}
                        </td>
                        <td className="px-3 py-2">{d.unique_users}</td>
                        <td className="px-3 py-2">
                          {formatUsd(notionalUsd ?? 0)}
                        </td>
                        <td className="px-3 py-2">
                          {formatUsd(routedUsd ?? 0)}
                        </td>
                        <td className="px-3 py-2">
                          {formatUsd(grossSurplusUsd ?? 0)}
                        </td>
                        <td className="px-3 py-2 tabular-nums">
                          {typeof d.xlm_usd === 'number' ? `$${d.xlm_usd.toFixed(4)}` : '—'}
                        </td>
                        <td className="px-3 py-2">{d.split_swap_count}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </details>
        </>
      )}
    </div>
  );
}

function ArbMetric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-[var(--text-muted)]">{label}</div>
      <div className="text-lg sm:text-xl font-semibold text-teal-200 mt-1.5 tabular-nums">
        {value}
      </div>
    </div>
  );
}

function KpiCard({
  label,
  value,
  hint,
  accent,
  delay = 0,
}: {
  label: string;
  value: string;
  hint: string;
  accent?: boolean;
  delay?: number;
}) {
  return (
    <div
      className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3.5 opacity-0 animate-[statsFadeIn_0.45s_ease_forwards]"
      style={{ animationDelay: `${delay}ms` }}
    >
      <div className="text-[11px] uppercase tracking-wide text-[var(--text-muted)]">{label}</div>
      <div
        className={`text-xl sm:text-2xl font-semibold mt-1.5 tracking-tight tabular-nums ${
          accent ? 'text-[var(--accent)]' : 'text-[var(--text-primary)]'
        }`}
      >
        {value}
      </div>
      <div className="mt-1 text-[10px] leading-snug text-[var(--text-muted)] sm:text-[11px]">
        {hint}
      </div>
    </div>
  );
}

function OpsItem({ label, value }: { label: string; value: ReactNode }) {
  return (
    <span className="inline-flex items-baseline gap-1.5">
      <span className="text-[var(--text-muted)]">{label}</span>
      <span className="text-[var(--text-secondary)] tabular-nums">{value}</span>
    </span>
  );
}

function shortTokenAddress(address: string): string {
  if (address.length <= 12) return address;
  return `${address.slice(0, 5)}…${address.slice(-4)}`;
}

function SharePanel({
  title,
  subtitle,
  entries,
  colorFn,
  displayName,
}: {
  title: string;
  subtitle: string;
  entries: [string, number][];
  colorFn: (name: string, index: number) => string;
  displayName?: (name: string) => string;
}) {
  const totalCount = entries.reduce((s, [, n]) => s + n, 0);
  const total = totalCount || 1;

  return (
    <section className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 px-4 sm:px-5 py-4">
      <div className="flex items-start justify-between gap-3 mb-0.5">
        <h2 className="text-[15px] font-medium text-[var(--text-primary)]">{title}</h2>
        {totalCount > 0 && (
          <span className="text-[12px] text-[var(--text-muted)] tabular-nums shrink-0">
            Total {totalCount.toLocaleString()}
          </span>
        )}
      </div>
      <p className="text-[12px] text-[var(--text-muted)] mt-0.5 mb-4">{subtitle}</p>
      {entries.length === 0 ? (
        <p className="text-[13px] text-[var(--text-muted)]">No data yet</p>
      ) : (
        <ul className="space-y-3">
          {entries.map(([name, count], i) => {
            const pct = (count / total) * 100;
            const color = colorFn(name, i);
            return (
              <li key={name}>
                <div className="flex items-center justify-between gap-2 text-[12px] mb-1.5">
                  <span className="text-[var(--text-secondary)] font-medium truncate" title={name}>
                    {displayName ? displayName(name) : name}
                  </span>
                  <span className="text-[var(--text-muted)] tabular-nums shrink-0">
                    {count.toLocaleString()} · {pct.toFixed(1)}%
                  </span>
                </div>
                <div className="h-2 rounded-full bg-[var(--surface-raised)] overflow-hidden">
                  <div
                    className="h-full rounded-full transition-[width] duration-700 ease-out"
                    style={{
                      width: `${Math.max(pct, 1.5)}%`,
                      background: color,
                      animationDelay: `${i * 80}ms`,
                    }}
                  />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

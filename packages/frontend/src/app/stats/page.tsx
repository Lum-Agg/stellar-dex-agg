'use client';

import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from 'react';
import Link from 'next/link';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

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
  round_trip_gross_surplus_usd?: number | null;
  split_swap_count: number;
  success_count: number;
  failed_count: number;
  xlm_usd?: number | null;
  total_amount_in_usd?: number | null;
  total_routed_dex_volume_usd?: number | null;
}

function dayRoutedUsd(d: DailyStats): number | null {
  if (typeof d.total_routed_dex_volume_usd === 'number' && Number.isFinite(d.total_routed_dex_volume_usd)) {
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

function shortDay(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  if (Number.isNaN(d.getTime())) return day.slice(5);
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' });
}

export default function StatsPage() {
  const [data, setData] = useState<StatsPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const statsRes = await fetch(`${API_URL}/api/v1/stats`, { cache: 'no-store' }).then((r) =>
          r.json(),
        );
        if (!statsRes.success) {
          throw new Error(statsRes.error || 'stats request failed');
        }
        if (!cancelled) setData(statsRes.data);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
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
    let txs = 0;
    let roundTrips = 0;
    let grossSurplusUsd = 0;
    let surplusPricedDays = 0;
    let surplusPricedRoundTrips = 0;
    let routedLegs = 0;
    let routedPricedLegs = 0;
    const dexTotals: Record<string, number> = {};
    const fnTotals: Record<string, number> = {};

    for (const d of days) {
      txs += d.tx_count;
      roundTrips += d.round_trip_count ?? 0;
      routedLegs += d.routed_leg_count ?? 0;
      routedPricedLegs += d.routed_priced_leg_count ?? 0;
      const nUsd = dayNotionalUsd(d);
      const rUsd = dayRoutedUsd(d);
      const surplusUsd = dayGrossSurplusUsd(d);
      if (nUsd != null) {
        notionalUsd += nUsd;
        usdCovered += 1;
      }
      if (rUsd != null) routedUsd += rUsd;
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
    }

    const venuesHit = Object.keys(dexTotals).length;
    const maxVol = Math.max(
      ...days.map((d) => Math.max(dayRoutedUsd(d) ?? 0, dayNotionalUsd(d) ?? 0)),
      1,
    );
    const maxTx = Math.max(...days.map((d) => d.tx_count), 1);
    const pricedDays = days.filter((d) => typeof d.xlm_usd === 'number');
    const avgXlmUsd =
      pricedDays.length > 0
        ? pricedDays.reduce((s, d) => s + (d.xlm_usd as number), 0) / pricedDays.length
        : null;

    return {
      days,
      notionalUsd: usdCovered > 0 ? notionalUsd : null,
      routedUsd: usdCovered > 0 || routedUsd > 0 ? routedUsd : null,
      avgXlmUsd,
      txs,
      routedPricingCoverage: routedLegs > 0 ? routedPricedLegs / routedLegs : null,
      roundTrips,
      grossSurplusUsd: surplusPricedDays > 0 ? grossSurplusUsd : null,
      averageGrossSurplusUsd:
        surplusPricedRoundTrips > 0 ? grossSurplusUsd / surplusPricedRoundTrips : null,
      surplusPricedDays,
      venuesHit,
      dexTotals,
      fnTotals,
      maxVol,
      maxTx,
    };
  }, [data]);

  return (
    <div className="max-w-5xl mx-auto space-y-8">
      <div className="flex flex-col sm:flex-row sm:items-end sm:justify-between gap-3">
        <div>
          <h1 className="text-2xl sm:text-3xl font-semibold tracking-tight text-[var(--text-primary)]">
            LumAgg-routed volume
          </h1>
          <p className="text-[13px] text-[var(--text-primary)]0 mt-2 leading-relaxed max-w-xl">
            Aggregator contract invocations only — volume LumAgg routed onto Stellar Soroban DEXes,
            not market-wide pool volume. Routed volume sums the actual input value processed by
            every executed DEX leg. Split routes use each pool&apos;s real allocation; multi-hop and
            round-trip legs are priced in their own input token (XLM = historical UTC day close;
            USDC = $1).
          </p>
        </div>
        {data && (
          <a
            href={`${API_URL}/api/v1/stats?format=csv`}
            className="self-start sm:self-auto text-[12px] px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--surface)]/80 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] transition-colors"
          >
            Export CSV
          </a>
        )}
      </div>

      {loading && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
          {Array.from({ length: 4 }).map((_, i) => (
            <div
              key={i}
              className="h-[88px] rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 animate-pulse"
            />
          ))}
        </div>
      )}

      {error && (
        <div className="text-sm text-amber-300/90 border border-amber-500/20 bg-amber-500/5 rounded-xl px-4 py-3">
          Stats unavailable: {error}. Configure <code className="text-[var(--text-secondary)]">INDEXER_DB_PATH</code>{' '}
          on the API server or use{' '}
          <code className="text-[var(--text-secondary)]">analytics-indexer export-daily</code>.
        </div>
      )}

      {data && derived && (
        <>
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <KpiCard
              label="Routed (USD)"
              value={derived.routedUsd != null ? formatUsd(derived.routedUsd) : '—'}
              hint={
                derived.routedPricingCoverage != null
                  ? `notional × avg hops · ${(derived.routedPricingCoverage * 100).toFixed(0)}% legs counted`
                  : 'notional × DEX hops'
              }
              accent
              delay={0}
            />
            <KpiCard
              label="Notional in (USD)"
              value={derived.notionalUsd != null ? formatUsd(derived.notionalUsd) : '—'}
              hint={
                derived.avgXlmUsd != null
                  ? `XLM day avg ≈ $${derived.avgXlmUsd.toFixed(4)}`
                  : 'priced inputs only'
              }
              delay={60}
            />
            <KpiCard
              label="Transactions"
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

          <section className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 overflow-hidden">
            <div className="px-4 sm:px-5 pt-4 pb-2 flex flex-col sm:flex-row sm:items-baseline sm:justify-between gap-2">
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text-primary)]">Daily volume</h2>
                <p className="text-[12px] text-[var(--text-primary)]0 mt-0.5">
                  Routed ≈ notional × hops (each DEX leg) · Notional = entry amount · cyan =
                  transactions
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--text-primary)]0">
                <span className="inline-flex items-center gap-1.5">
                  <span className="w-2.5 h-2.5 rounded-sm bg-[var(--accent)]/80" />
                  Routed (USD)
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <span className="w-2.5 h-2.5 rounded-sm bg-zinc-400/80" />
                  Notional (USD)
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <span className="w-2.5 h-0.5 bg-teal-300/90" />
                  Transactions
                </span>
              </div>
            </div>
            <VolumeTrendChart
              days={derived.days}
              maxVol={derived.maxVol}
              maxTx={derived.maxTx}
            />
          </section>

          <section className="rounded-xl border border-teal-400/20 bg-[linear-gradient(115deg,rgba(45,212,191,0.08),rgba(15,23,42,0.18))] px-4 sm:px-5 py-4">
            <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2">
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text-primary)]">
                  Arbitrage execution
                </h2>
                <p className="text-[12px] text-[var(--text-primary)]0 mt-1 max-w-2xl">
                  Successful on-chain round trips. Gross surplus is the actual base token returned
                  minus the base token supplied; it excludes transaction fees and is not net P&amp;L.
                  See the{' '}
                  <Link href="/arbitrage" className="text-teal-300/90 hover:text-teal-200 underline underline-offset-2">
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
                value={
                  derived.grossSurplusUsd != null ? formatUsd(derived.grossSurplusUsd) : '—'
                }
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

          <div className="grid lg:grid-cols-2 gap-4">
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
          </div>

          <div className="flex flex-wrap gap-x-5 gap-y-2 rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/40 px-4 py-3 text-[12px] text-[var(--text-primary)]0">
            <OpsItem label="Cursor ledger" value={data.cursor_ledger?.toLocaleString() ?? '—'} />
            <OpsItem label="Days indexed" value={String(data.daily.length)} />
            <OpsItem
              label="USD pricing"
              value={data.usd_pricing ? 'per-token day close' : '—'}
            />
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

          <details className="group rounded-xl border border-[var(--border)] bg-[var(--surface)]/50 open:bg-[var(--surface)]/60">
            <summary className="cursor-pointer list-none px-4 sm:px-5 py-3.5 flex items-center justify-between text-[14px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] [&::-webkit-details-marker]:hidden">
              <span className="font-medium">Daily rollup</span>
              <span className="text-[12px] text-[var(--text-primary)]0">
                <span className="group-open:hidden">Show table</span>
                <span className="hidden group-open:inline">Hide</span>
              </span>
            </summary>
            <div className="overflow-x-auto border-t border-[var(--border)]">
              <table className="w-full text-[12px] text-left">
                <thead className="bg-[var(--bg-0)]/50 text-[var(--text-primary)]0">
                  <tr>
                    <th className="px-3 py-2 font-medium">Day</th>
                    <th className="px-3 py-2 font-medium">Transactions</th>
                    <th className="px-3 py-2 font-medium">Users</th>
                    <th className="px-3 py-2 font-medium">Notional USD</th>
                    <th className="px-3 py-2 font-medium">Routed USD</th>
                    <th className="px-3 py-2 font-medium">RT gross USD</th>
                    <th className="px-3 py-2 font-medium">XLM/USD</th>
                    <th className="px-3 py-2 font-medium">Split</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--border)]">
                  {[...derived.days].reverse().map((d) => {
                    const notionalUsd = dayNotionalUsd(d);
                    const routedUsd = dayRoutedUsd(d);
                    const grossSurplusUsd = dayGrossSurplusUsd(d);
                    return (
                      <tr key={d.day} className="text-[var(--text-secondary)]">
                        <td className="px-3 py-2 whitespace-nowrap">{d.day}</td>
                        <td className="px-3 py-2">{d.tx_count}</td>
                        <td className="px-3 py-2">{d.unique_users}</td>
                        <td className="px-3 py-2">
                          {notionalUsd != null ? formatUsd(notionalUsd) : '—'}
                        </td>
                        <td className="px-3 py-2">
                          {routedUsd != null ? formatUsd(routedUsd) : '—'}
                        </td>
                        <td className="px-3 py-2">
                          {grossSurplusUsd != null ? formatUsd(grossSurplusUsd) : '—'}
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
      <div className="text-[11px] uppercase tracking-wide text-[var(--text-primary)]0">{label}</div>
      <div
        className={`text-xl sm:text-2xl font-semibold mt-1.5 tracking-tight tabular-nums ${
          accent ? 'text-[var(--accent)]' : 'text-[var(--text-primary)]'
        }`}
      >
        {value}
      </div>
      <div className="text-[11px] text-[var(--text-muted)] mt-1 truncate">{hint}</div>
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

function SharePanel({
  title,
  subtitle,
  entries,
  colorFn,
}: {
  title: string;
  subtitle: string;
  entries: [string, number][];
  colorFn: (name: string, index: number) => string;
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
                  <span className="text-[var(--text-secondary)] font-medium truncate">{name}</span>
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

function VolumeTrendChart({
  days,
  maxVol,
  maxTx,
}: {
  days: DailyStats[];
  maxVol: number;
  maxTx: number;
}) {
  const w = 640;
  const h = 248;
  const padL = 8;
  const padR = 8;
  const padT = 42;
  const padB = 44;
  const chartW = w - padL - padR;
  const chartH = h - padT - padB;
  const n = Math.max(days.length, 1);
  const gap = 0.22;
  const slot = chartW / n;
  const pairW = slot * (1 - gap);
  const inner = 3;
  const barW = Math.max((pairW - inner) / 2, 6);

  const txPoints = days.map((d, i) => {
    const x = padL + slot * i + slot / 2;
    const y = padT + chartH * (1 - d.tx_count / maxTx);
    return `${x},${y}`;
  });

  return (
    <div className="px-2 sm:px-3 pb-3">
      <svg
        viewBox={`0 0 ${w} ${h}`}
        className="w-full h-[240px] sm:h-[260px]"
        role="img"
        aria-label="Daily routed and notional volume with transaction counts"
      >
        {[0.25, 0.5, 0.75, 1].map((t) => {
          const y = padT + chartH * (1 - t);
          return (
            <line
              key={t}
              x1={padL}
              x2={w - padR}
              y1={y}
              y2={y}
              stroke="rgba(255,255,255,0.04)"
              strokeWidth={1}
            />
          );
        })}

        {days.map((d, i) => {
          const routed = dayRoutedUsd(d) ?? 0;
          const notional = dayNotionalUsd(d) ?? 0;
          const pairX = padL + slot * i + (slot - pairW) / 2;
          const routedH = (routed / maxVol) * chartH;
          const notionalH = (notional / maxVol) * chartH;
          const routedY = padT + chartH - routedH;
          const notionalY = padT + chartH - notionalH;
          const cx = padL + slot * i + slot / 2;
          const labelTop = Math.min(routedY, notionalY);
          return (
            <g
              key={d.day}
              className="opacity-0 animate-[statsFadeIn_0.5s_ease_forwards]"
              style={{ animationDelay: `${i * 70}ms` } as CSSProperties}
            >
              <defs>
                <linearGradient id={`routedGrad-${i}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="#3dd6c6" stopOpacity="0.95" />
                  <stop offset="100%" stopColor="#2fc4b4" stopOpacity="0.55" />
                </linearGradient>
                <linearGradient id={`notionalGrad-${i}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="#d4d4d8" stopOpacity="0.85" />
                  <stop offset="100%" stopColor="#71717a" stopOpacity="0.5" />
                </linearGradient>
              </defs>
              <rect
                x={pairX}
                y={routedY}
                width={barW}
                height={Math.max(routedH, routed > 0 ? 2 : 0)}
                rx={3}
                fill={`url(#routedGrad-${i})`}
              >
                <title>
                  {d.day}: routed {formatUsd(routed)} · notional {formatUsd(notional)} ·{' '}
                  {d.tx_count} transactions
                </title>
              </rect>
              <rect
                x={pairX + barW + inner}
                y={notionalY}
                width={barW}
                height={Math.max(notionalH, notional > 0 ? 2 : 0)}
                rx={3}
                fill={`url(#notionalGrad-${i})`}
              >
                <title>
                  {d.day}: notional {formatUsd(notional)} · routed {formatUsd(routed)} ·{' '}
                  {d.tx_count} transactions
                </title>
              </rect>
              <text
                x={cx}
                y={labelTop - 20}
                textAnchor="middle"
                fill="#7dd3c8"
                fontSize="10"
                className="tabular-nums"
              >
                {routed > 0 ? formatUsd(routed) : '—'}
              </text>
              <text
                x={cx}
                y={labelTop - 8}
                textAnchor="middle"
                fill="#9ca3af"
                fontSize="10"
                className="tabular-nums"
              >
                {notional > 0 ? formatUsd(notional) : '—'}
              </text>
              <text x={cx} y={h - 20} textAnchor="middle" fill="#6b7280" fontSize="10">
                {shortDay(d.day)}
              </text>
              <text
                x={cx}
                y={h - 6}
                textAnchor="middle"
                fill="#99f6e4"
                fontSize="11"
                fontWeight={600}
                className="tabular-nums"
              >
                {d.tx_count} tx
              </text>
            </g>
          );
        })}

        {days.length > 1 && (
          <polyline
            fill="none"
            stroke="#5eead4"
            strokeWidth={2}
            strokeLinejoin="round"
            strokeLinecap="round"
            points={txPoints.join(' ')}
            opacity={0.85}
          />
        )}
        {days.map((d, i) => {
          const x = padL + slot * i + slot / 2;
          const y = padT + chartH * (1 - d.tx_count / maxTx);
          return (
            <circle
              key={`tx-${d.day}`}
              cx={x}
              cy={y}
              r={3.5}
              fill="#99f6e4"
              stroke="#0f766e"
              strokeWidth={1}
            >
              <title>
                {d.day}: {d.tx_count} transactions
              </title>
            </circle>
          );
        })}
      </svg>
    </div>
  );
}

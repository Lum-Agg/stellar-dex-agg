'use client';

import { useEffect, useMemo, useState } from 'react';
import Link from 'next/link';
import { NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { useTokenList } from '@/components/TokenSelector';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

const XLM_SAC = NATIVE_CONTRACT;
const USDC_SAC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

interface RoundTripSurplus {
  base_token: string;
  tx_count: number;
  amount_in: string | number;
  gross_surplus: string | number;
  gross_surplus_usd?: number | null;
}

interface DailyStats {
  day: string;
  round_trip_count?: number;
  round_trip_by_token?: RoundTripSurplus[];
  round_trip_gross_surplus_usd?: number | null;
  xlm_usd?: number | null;
}

interface StatsPayload {
  daily: DailyStats[];
}

interface DailyProfit {
  day: string;
  xlm: bigint;
  usdc: bigint;
  xlmTx: number;
  usdcTx: number;
}

type ProfitRange = '30D' | '90D' | 'ALL';

interface RoundTripItem {
  tx_hash: string;
  ledger: number;
  created_at: number;
  status: string;
  base_token?: string | null;
  bridge_token?: string | null;
  amount_in: string;
  amount_out?: string | null;
  gross_surplus?: string | null;
  is_split: boolean;
}

interface ArbitrageStatusCounts {
  success_count?: number;
  failed_count?: number;
}

function formatUsd(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n >= 1_000_000) return `$${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `$${(n / 1_000).toFixed(2)}K`;
  return `$${n.toLocaleString(undefined, { maximumFractionDigits: 2 })}`;
}

function tokenLabel(
  contract: string | null | undefined,
  labels: ReadonlyMap<string, string>,
): string {
  if (!contract) return '—';
  const symbol = labels.get(contract);
  if (symbol) return symbol;
  if (contract.length <= 12) return contract;
  return `${contract.slice(0, 4)}…${contract.slice(-4)}`;
}

function formatAmount(raw: string | null | undefined, decimals = 7): string {
  if (raw == null || raw === '') return '—';
  try {
    const neg = raw.startsWith('-');
    const digits = neg ? raw.slice(1) : raw;
    if (!/^\d+$/.test(digits)) return raw;
    const padded = digits.padStart(decimals + 1, '0');
    const whole = padded.slice(0, -decimals) || '0';
    const frac = padded.slice(-decimals).replace(/0+$/, '');
    const body = frac
      ? `${Number(whole).toLocaleString()}.${frac}`
      : Number(whole).toLocaleString();
    return neg ? `−${body}` : body;
  } catch {
    return raw;
  }
}

function utcDayString(d = new Date()): string {
  return d.toISOString().slice(0, 10);
}

function daysAgoUtc(n: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - n);
  return d.toISOString().slice(0, 10);
}

function shortHash(hash: string): string {
  if (hash.length <= 14) return hash;
  return `${hash.slice(0, 6)}…${hash.slice(-6)}`;
}

function formatWhen(ts: number): string {
  const d = new Date(ts * 1000);
  if (Number.isNaN(d.getTime())) return '—';
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  });
}

function toRawBigInt(raw: string | number): bigint | null {
  try {
    if (typeof raw === 'number') {
      if (!Number.isFinite(raw)) return null;
      return BigInt(Math.trunc(raw));
    }
    if (!/^-?\d+$/.test(raw)) return null;
    return BigInt(raw);
  } catch {
    return null;
  }
}

function formatSurplusSigned(raw: string | number | bigint, symbol: string): string {
  const asString = typeof raw === 'bigint' ? raw.toString() : String(raw);
  const formatted = formatAmount(asString);
  if (formatted === '—') return '—';
  const neg = asString.startsWith('-');
  return `${neg ? '' : '+'}${formatted} ${symbol}`;
}

function formatDay(day: string): string {
  const date = new Date(`${day}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return day;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
}

export default function ArbitragePage() {
  const tokens = useTokenList();
  const [stats, setStats] = useState<StatsPayload | null>(null);
  const [trips, setTrips] = useState<RoundTripItem[]>([]);
  const [statusCounts, setStatusCounts] = useState<ArbitrageStatusCounts>({});
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [profitRange, setProfitRange] = useState<ProfitRange>('30D');
  const [showDailyTable, setShowDailyTable] = useState(false);
  const [hoveredProfitDay, setHoveredProfitDay] = useState<string | null>(null);
  const tokenLabels = useMemo(
    () => new Map(tokens.map((token) => [token.id, token.symbol])),
    [tokens],
  );

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [statsRes, arbRes] = await Promise.all([
          fetch(`${API_URL}/api/v1/stats`, { cache: 'no-store' }).then((r) => r.json()),
          fetch(`${API_URL}/api/v1/arbitrage?limit=25`, { cache: 'no-store' }).then((r) =>
            r.json(),
          ),
        ]);
        if (!statsRes.success) throw new Error(statsRes.error || 'stats request failed');
        if (!arbRes.success) throw new Error(arbRes.error || 'arbitrage request failed');
        if (cancelled) return;
        setStats(statsRes.data);
        setTrips(arbRes.data?.round_trips ?? []);
        setStatusCounts({
          success_count: arbRes.data?.success_count,
          failed_count: arbRes.data?.failed_count,
        });
        setNextCursor(arbRes.data?.next_cursor ?? null);
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

  const summary = useMemo(() => {
    if (!stats) return null;
    const today = utcDayString();
    const from7 = daysAgoUtc(6);
    const days = stats.daily.filter((d) => d.day >= from7 && d.day <= today);

    let todayCount = 0;
    let todayUsd: number | null = null;
    let weekCount = 0;
    let weekUsd = 0;
    let weekUsdCovered = 0;

    let xlmSurplus = BigInt(0);
    let usdcSurplus = BigInt(0);
    let xlmTx = 0;
    let usdcTx = 0;
    let daySpan = 0;

    for (const d of stats.daily) {
      const hasRoundTrip =
        (d.round_trip_count ?? 0) > 0 || (d.round_trip_by_token?.length ?? 0) > 0;
      if (hasRoundTrip) daySpan += 1;
      for (const row of d.round_trip_by_token ?? []) {
        const surplus = toRawBigInt(row.gross_surplus);
        if (surplus == null) continue;
        if (row.base_token === XLM_SAC) {
          xlmSurplus += surplus;
          xlmTx += row.tx_count;
        } else if (row.base_token === USDC_SAC) {
          usdcSurplus += surplus;
          usdcTx += row.tx_count;
        }
      }
    }

    for (const d of days) {
      const count = d.round_trip_count ?? 0;
      weekCount += count;
      if (typeof d.round_trip_gross_surplus_usd === 'number') {
        weekUsd += d.round_trip_gross_surplus_usd;
        weekUsdCovered += 1;
      }
      if (d.day === today) {
        todayCount = count;
        todayUsd =
          typeof d.round_trip_gross_surplus_usd === 'number'
            ? d.round_trip_gross_surplus_usd
            : null;
      }
    }

    return {
      todayCount,
      todayUsd,
      weekCount,
      weekUsd: weekUsdCovered > 0 ? weekUsd : null,
      xlmSurplus,
      usdcSurplus,
      xlmTx,
      usdcTx,
      daySpan,
    };
  }, [stats]);

  const dailyProfit = useMemo<DailyProfit[]>(() => {
    if (!stats) return [];
    return stats.daily
      .map((day) => {
        let xlm = BigInt(0);
        let usdc = BigInt(0);
        let xlmTx = 0;
        let usdcTx = 0;
        for (const row of day.round_trip_by_token ?? []) {
          const surplus = toRawBigInt(row.gross_surplus);
          if (surplus == null) continue;
          if (row.base_token === XLM_SAC) {
            xlm += surplus;
            xlmTx += row.tx_count;
          } else if (row.base_token === USDC_SAC) {
            usdc += surplus;
            usdcTx += row.tx_count;
          }
        }
        return { day: day.day, xlm, usdc, xlmTx, usdcTx };
      })
      .filter((day) => day.xlm !== BigInt(0) || day.usdc !== BigInt(0));
  }, [stats]);

  const visibleDailyProfit = useMemo(() => {
    if (profitRange === 'ALL') return dailyProfit;
    const days = profitRange === '30D' ? 30 : 90;
    return dailyProfit.slice(-days);
  }, [dailyProfit, profitRange]);

  const chartMax = useMemo(() => {
    const zero = BigInt(0);
    const values = visibleDailyProfit.flatMap((day) => [
      day.xlm < zero ? -day.xlm : day.xlm,
      day.usdc < zero ? -day.usdc : day.usdc,
    ]);
    return values.reduce((max, value) => (value > max ? value : max), BigInt(1));
  }, [visibleDailyProfit]);

  async function loadMore() {
    if (!nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const res = await fetch(
        `${API_URL}/api/v1/arbitrage?limit=25&cursor=${encodeURIComponent(nextCursor)}`,
        { cache: 'no-store' },
      ).then((r) => r.json());
      if (!res.success) throw new Error(res.error || 'arbitrage request failed');
      setTrips((prev) => [...prev, ...(res.data?.round_trips ?? [])]);
      setNextCursor(res.data?.next_cursor ?? null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingMore(false);
    }
  }

  return (
    <div className="max-w-5xl mx-auto space-y-8">
      <div className="flex flex-col sm:flex-row sm:items-end sm:justify-between gap-3">
        <div>
          <h1 className="text-2xl sm:text-3xl font-semibold tracking-tight text-[var(--text-primary)]">
            Arbitrage
          </h1>
          <p className="text-[13px] text-[var(--text-muted)] mt-2 leading-relaxed max-w-2xl">
            Successful on-chain round trips executed through LumAgg. A vault holds principal;
            callers pay gas. Gross surplus is base token returned minus base supplied — fees are not
            deducted, so this is not net P&amp;L.
          </p>
        </div>
        <Link
          href="/stats"
          className="self-start sm:self-auto text-[12px] px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--surface)]/80 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] transition-colors"
        >
          Full stats →
        </Link>
      </div>

      {loading && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            {Array.from({ length: 4 }).map((_, i) => (
              <div
                key={i}
                className="h-[88px] rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 animate-pulse"
              />
            ))}
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {Array.from({ length: 2 }).map((_, i) => (
              <div
                key={i}
                className="h-[88px] rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 animate-pulse"
              />
            ))}
          </div>
        </div>
      )}

      {error && (
        <div className="text-sm text-amber-300/90 border border-amber-500/20 bg-amber-500/5 rounded-xl px-4 py-3">
          Arbitrage data unavailable: {error}
        </div>
      )}

      {summary && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <KpiCard
              label="Today"
              value={summary.todayCount.toLocaleString()}
              hint="successful round trips (UTC)"
              accent
              delay={0}
            />
            <KpiCard
              label="Today surplus"
              value={summary.todayUsd != null ? formatUsd(summary.todayUsd) : '—'}
              hint="gross, USD-priced"
              delay={60}
            />
            <KpiCard
              label="Last 7 days"
              value={summary.weekCount.toLocaleString()}
              hint="successful round trips"
              delay={120}
            />
            <KpiCard
              label="7d surplus"
              value={summary.weekUsd != null ? formatUsd(summary.weekUsd) : '—'}
              hint="gross, USD-priced"
              delay={180}
            />
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <KpiCard
              label="All-time XLM surplus"
              value={formatSurplusSigned(summary.xlmSurplus, 'XLM')}
              hint={`${summary.xlmTx.toLocaleString()} round trips · ${summary.daySpan} indexed days · gross`}
              delay={220}
            />
            <KpiCard
              label="All-time USDC surplus"
              value={formatSurplusSigned(summary.usdcSurplus, 'USDC')}
              hint={`${summary.usdcTx.toLocaleString()} round trips · ${summary.daySpan} indexed days · gross`}
              delay={260}
            />
          </div>
        </div>
      )}

      <section className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 overflow-hidden">
        <div className="px-4 sm:px-5 pt-4 pb-3 flex flex-col sm:flex-row sm:items-end sm:justify-between gap-2">
          <div>
            <h2 className="text-[15px] font-medium text-[var(--text-primary)]">
              Daily gross surplus
            </h2>
            <p className="text-[12px] text-[var(--text-muted)] mt-0.5">
              Successful round trips · gross surplus before network fees
            </p>
          </div>
          <div className="flex flex-wrap items-center justify-between gap-3 text-[11px] text-[var(--text-muted)]">
            <div className="flex items-center rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/50 p-0.5">
              {(['30D', '90D', 'ALL'] as const).map((range) => (
                <button
                  key={range}
                  type="button"
                  onClick={() => setProfitRange(range)}
                  className={`rounded-md px-2 py-1 transition-colors ${
                    profitRange === range
                      ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
                      : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
                  }`}
                >
                  {range === 'ALL' ? 'All' : range}
                </button>
              ))}
            </div>
            <div className="flex items-center gap-3">
              <span className="inline-flex items-center gap-1.5">
                <i className="h-2 w-2 rounded-full bg-teal-300" />
                XLM
              </span>
              <span className="inline-flex items-center gap-1.5">
                <i className="h-2 w-2 rounded-full bg-sky-300" />
                USDC
              </span>
            </div>
          </div>
        </div>

        {dailyProfit.length === 0 ? (
          <p className="border-t border-[var(--border)] px-4 sm:px-5 py-5 text-[13px] text-[var(--text-muted)]">
            No daily surplus has been indexed yet.
          </p>
        ) : (
          <>
            <div className="border-t border-[var(--border)] px-4 sm:px-5 pt-5 pb-4">
              <div className="flex items-end gap-2 sm:gap-3 h-44">
                {visibleDailyProfit.map((day) => {
                  const zero = BigInt(0);
                  const xlmHeight = Number(
                    ((day.xlm < zero ? -day.xlm : day.xlm) * BigInt(100)) / chartMax,
                  );
                  const usdcHeight = Number(
                    ((day.usdc < zero ? -day.usdc : day.usdc) * BigInt(100)) / chartMax,
                  );
                  return (
                    <div
                      key={day.day}
                      className="relative min-w-0 flex-1 h-full flex flex-col justify-end gap-2 group"
                      onMouseEnter={() => setHoveredProfitDay(day.day)}
                      onMouseLeave={() => setHoveredProfitDay(null)}
                    >
                      {hoveredProfitDay === day.day && (
                        <div className="pointer-events-none absolute bottom-8 left-1/2 z-10 w-36 -translate-x-1/2 rounded-lg border border-[var(--border-strong)] bg-[var(--surface-raised)] px-3 py-2 text-[11px] shadow-xl">
                          <div className="font-medium text-[var(--text-primary)]">{day.day}</div>
                          <div className="mt-1 flex justify-between gap-3 text-teal-200">
                            <span>XLM</span>
                            <span className="tabular-nums">
                              {formatSurplusSigned(day.xlm, 'XLM')}
                            </span>
                          </div>
                          <div className="flex justify-between gap-3 text-sky-200">
                            <span>USDC</span>
                            <span className="tabular-nums">
                              {formatSurplusSigned(day.usdc, 'USDC')}
                            </span>
                          </div>
                          <div className="mt-1 border-t border-[var(--border)] pt-1 text-[var(--text-muted)]">
                            {day.xlmTx + day.usdcTx} round trips
                          </div>
                        </div>
                      )}
                      <div className="flex-1 flex items-end justify-center gap-1">
                        <div
                          className="w-full max-w-5 rounded-t bg-teal-300/85 transition-all group-hover:bg-teal-200"
                          style={{
                            height: `${Math.max(xlmHeight, day.xlm !== BigInt(0) ? 3 : 0)}%`,
                          }}
                          title={`${day.day}: ${formatSurplusSigned(day.xlm, 'XLM')}`}
                        />
                        <div
                          className="w-full max-w-5 rounded-t bg-sky-300/85 transition-all group-hover:bg-sky-200"
                          style={{
                            height: `${Math.max(usdcHeight, day.usdc !== BigInt(0) ? 3 : 0)}%`,
                          }}
                          title={`${day.day}: ${formatSurplusSigned(day.usdc, 'USDC')}`}
                        />
                      </div>
                      <span className="truncate text-center text-[10px] text-[var(--text-muted)]">
                        {formatDay(day.day)}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
            <div className="border-t border-[var(--border)]">
              <button
                type="button"
                onClick={() => setShowDailyTable((shown) => !shown)}
                className="flex w-full items-center justify-between px-4 sm:px-5 py-3 text-left text-[12px] text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
                aria-expanded={showDailyTable}
              >
                <span>{showDailyTable ? 'Hide daily table' : 'Show daily table'}</span>
                <span aria-hidden="true">{showDailyTable ? '⌃' : '⌄'}</span>
              </button>
            </div>
            {showDailyTable && (
              <div className="overflow-x-auto border-t border-[var(--border)]">
                <table className="w-full text-[12px] text-left">
                  <thead className="bg-[var(--bg-0)]/50 text-[var(--text-muted)]">
                    <tr>
                      <th className="px-4 py-2.5 font-medium">Day (UTC)</th>
                      <th className="px-4 py-2.5 font-medium text-right">XLM surplus</th>
                      <th className="px-4 py-2.5 font-medium text-right">USDC surplus</th>
                      <th className="px-4 py-2.5 font-medium text-right">Round trips</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[var(--border)]">
                    {[...visibleDailyProfit].reverse().map((day) => (
                      <tr key={day.day} className="text-[var(--text-secondary)]">
                        <td className="px-4 py-2.5 whitespace-nowrap text-[var(--text-muted)]">
                          {day.day}
                        </td>
                        <td className="px-4 py-2.5 text-right tabular-nums whitespace-nowrap text-teal-200">
                          {formatSurplusSigned(day.xlm, 'XLM')}
                        </td>
                        <td className="px-4 py-2.5 text-right tabular-nums whitespace-nowrap text-sky-200">
                          {formatSurplusSigned(day.usdc, 'USDC')}
                        </td>
                        <td className="px-4 py-2.5 text-right tabular-nums whitespace-nowrap">
                          {(day.xlmTx + day.usdcTx).toLocaleString()}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </section>

      <section className="rounded-xl border border-teal-400/20 bg-[linear-gradient(115deg,rgba(45,212,191,0.08),rgba(15,23,42,0.18))] px-4 sm:px-5 py-4">
        <h2 className="text-[15px] font-medium text-[var(--text-primary)]">How it works</h2>
        <p className="text-[12px] text-[var(--text-muted)] mt-1.5 leading-relaxed max-w-3xl">
          When venue prices diverge, LumAgg callers run a two-leg round trip against the vault
          balance (typically XLM → bridge → XLM). Only confirmed successful transactions are shown
          here — not quotes, simulations, or rejected opportunities.
        </p>
      </section>

      <section className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        <StatusCard
          label="Confirmed success"
          value={statusCounts.success_count?.toLocaleString() ?? '—'}
          tone="success"
        />
        <StatusCard
          label="Confirmed failed"
          value={statusCounts.failed_count?.toLocaleString() ?? '—'}
          tone="failed"
        />
        <div className="col-span-2 sm:col-span-1 rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3 text-[11px] text-[var(--text-muted)]">
          <div className="text-[var(--text-secondary)]">Status scope</div>
          <div className="mt-1 leading-relaxed">Indexed on-chain round trips only. Bot broadcasts still awaiting confirmation are not included.</div>
        </div>
      </section>

      <section className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 overflow-hidden">
        <div className="px-4 sm:px-5 pt-4 pb-3 flex items-baseline justify-between gap-2">
          <div>
            <h2 className="text-[15px] font-medium text-[var(--text-primary)]">Recent trades</h2>
            <p className="text-[12px] text-[var(--text-muted)] mt-0.5">
              On-chain actuals · newest first
            </p>
          </div>
          <a
            href={`${API_URL}/api/v1/arbitrage`}
            className="text-[11px] text-[var(--text-muted)] hover:text-[var(--text-secondary)] underline underline-offset-2"
            target="_blank"
            rel="noreferrer"
          >
            API
          </a>
        </div>

        {loading ? (
          <div className="px-4 sm:px-5 pb-5 space-y-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <div
                key={i}
                className="h-10 rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/40 animate-pulse"
              />
            ))}
          </div>
        ) : trips.length === 0 ? (
          <p className="px-4 sm:px-5 pb-5 text-[13px] text-[var(--text-muted)]">
            No successful round trips indexed yet.
          </p>
        ) : (
          <>
            <div className="overflow-x-auto border-t border-[var(--border)]">
              <table className="w-full text-[12px] text-left">
                <thead className="bg-[var(--bg-0)]/50 text-[var(--text-muted)]">
                  <tr>
                    <th className="px-3 sm:px-4 py-2.5 font-medium">When</th>
                    <th className="px-3 sm:px-4 py-2.5 font-medium">Tx</th>
                    <th className="px-3 sm:px-4 py-2.5 font-medium">Route</th>
                    <th className="px-3 sm:px-4 py-2.5 font-medium text-right">Amount in</th>
                    <th className="px-3 sm:px-4 py-2.5 font-medium text-right">Surplus</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--border)]">
                  {trips.map((t) => {
                    const base = tokenLabel(t.base_token, tokenLabels);
                    const bridge = tokenLabel(t.bridge_token, tokenLabels);
                    return (
                      <tr key={t.tx_hash} className="text-[var(--text-secondary)]">
                        <td className="px-3 sm:px-4 py-2.5 whitespace-nowrap text-[var(--text-muted)]">
                          {formatWhen(t.created_at)}
                        </td>
                        <td className="px-3 sm:px-4 py-2.5 whitespace-nowrap font-[family-name:var(--font-mono)]">
                          <a
                            href={`https://stellar.expert/explorer/public/tx/${t.tx_hash}`}
                            target="_blank"
                            rel="noreferrer"
                            className="text-teal-300/90 hover:text-teal-200 underline underline-offset-2"
                          >
                            {shortHash(t.tx_hash)}
                          </a>
                        </td>
                        <td className="px-3 sm:px-4 py-2.5 whitespace-nowrap">
                          <span className="text-[var(--text-primary)]">{base}</span>
                          <span className="text-[var(--text-muted)] mx-1">→</span>
                          <span>{bridge}</span>
                          <span className="text-[var(--text-muted)] mx-1">→</span>
                          <span className="text-[var(--text-primary)]">{base}</span>
                          {t.is_split && (
                            <span className="ml-2 text-[10px] uppercase tracking-wide text-[var(--text-muted)]">
                              split
                            </span>
                          )}
                        </td>
                        <td className="px-3 sm:px-4 py-2.5 text-right tabular-nums whitespace-nowrap">
                          {formatAmount(t.amount_in)} {base}
                        </td>
                        <td className="px-3 sm:px-4 py-2.5 text-right tabular-nums whitespace-nowrap text-teal-200">
                          {t.gross_surplus != null ? (
                            <>
                              {t.gross_surplus.startsWith('-') ? '' : '+'}
                              {formatAmount(t.gross_surplus)} {base}
                            </>
                          ) : (
                            '—'
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {nextCursor && (
              <div className="px-4 sm:px-5 py-3 border-t border-[var(--border)]">
                <button
                  type="button"
                  onClick={loadMore}
                  disabled={loadingMore}
                  className="text-[12px] px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/50 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] transition-colors disabled:opacity-50"
                >
                  {loadingMore ? 'Loading…' : 'Load more'}
                </button>
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}

function StatusCard({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: 'success' | 'failed';
}) {
  return (
    <div className="rounded-xl border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3">
      <div className="text-[11px] text-[var(--text-muted)]">{label}</div>
      <div className={`mt-1 text-xl tabular-nums ${tone === 'success' ? 'text-teal-200' : 'text-rose-200'}`}>
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
      <div className="text-[11px] text-[var(--text-muted)] mt-1 truncate">{hint}</div>
    </div>
  );
}

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

export default function ArbitragePage() {
  const tokens = useTokenList();
  const [stats, setStats] = useState<StatsPayload | null>(null);
  const [trips, setTrips] = useState<RoundTripItem[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
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

      <section className="rounded-xl border border-teal-400/20 bg-[linear-gradient(115deg,rgba(45,212,191,0.08),rgba(15,23,42,0.18))] px-4 sm:px-5 py-4">
        <h2 className="text-[15px] font-medium text-[var(--text-primary)]">How it works</h2>
        <p className="text-[12px] text-[var(--text-muted)] mt-1.5 leading-relaxed max-w-3xl">
          When venue prices diverge, LumAgg callers run a two-leg round trip against the vault
          balance (typically XLM → bridge → XLM). Only confirmed successful transactions are shown
          here — not quotes, simulations, or rejected opportunities.
        </p>
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

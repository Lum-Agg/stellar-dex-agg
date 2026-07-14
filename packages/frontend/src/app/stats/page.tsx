'use client';

import { useEffect, useState } from 'react';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

interface DailyStats {
  day: string;
  tx_count: number;
  unique_users: number;
  total_amount_in: string | number;
  by_function: Record<string, number>;
  by_dex: Record<string, number>;
  split_swap_count: number;
  success_count: number;
  failed_count: number;
}

interface StatsPayload {
  db_path: string;
  invocation_count: number;
  cursor_ledger: number | null;
  oldest_created_at: number | null;
  daily: DailyStats[];
}

function stroopsToXlm(v: string | number): string {
  const n = Number(v);
  if (!Number.isFinite(n)) return '—';
  return (n / 1e7).toLocaleString(undefined, { maximumFractionDigits: 2 });
}

export default function StatsPage() {
  const [data, setData] = useState<StatsPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`${API_URL}/api/v1/stats`, { cache: 'no-store' });
        const json = await res.json();
        if (!json.success) {
          throw new Error(json.error || `HTTP ${res.status}`);
        }
        if (!cancelled) setData(json.data);
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

  const totals = data?.daily.reduce(
    (acc, d) => ({
      txs: acc.txs + d.tx_count,
      users: acc.users + d.unique_users,
      volume: acc.volume + Number(d.total_amount_in),
      legs: acc.legs + Object.values(d.by_dex).reduce((s, n) => s + n, 0),
    }),
    { txs: 0, users: 0, volume: 0, legs: 0 }
  );

  const dexTotals = data?.daily.reduce<Record<string, number>>((acc, d) => {
    for (const [k, v] of Object.entries(d.by_dex)) {
      acc[k] = (acc[k] || 0) + v;
    }
    return acc;
  }, {});

  return (
    <div className="max-w-3xl mx-auto space-y-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight text-zinc-50">On-chain stats</h1>
        <p className="text-[13px] text-zinc-500 mt-2 leading-relaxed">
          Aggregator contract invocations indexed from Soroban events. Volume is notional XLM in
          (stroops); arb round-trips included.
        </p>
      </div>

      {loading && <p className="text-sm text-zinc-500">Loading…</p>}
      {error && (
        <div className="text-sm text-amber-300/90 border border-amber-500/20 bg-amber-500/5 rounded-lg px-4 py-3">
          Stats unavailable: {error}. Configure <code className="text-zinc-300">INDEXER_DB_PATH</code>{' '}
          on the API server or use{' '}
          <code className="text-zinc-300">analytics-indexer export-daily</code>.
        </div>
      )}

      {data && (
        <>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            {[
              ['Invocations', data.invocation_count.toLocaleString()],
              ['Days indexed', data.daily.length.toString()],
              ['Cursor ledger', data.cursor_ledger?.toLocaleString() ?? '—'],
              ['Volume (Σ days)', totals ? `${stroopsToXlm(totals.volume)} XLM` : '—'],
            ].map(([label, value]) => (
              <div
                key={label}
                className="rounded-xl border border-white/[0.08] bg-zinc-900/40 px-4 py-3"
              >
                <div className="text-[11px] uppercase tracking-wide text-zinc-500">{label}</div>
                <div className="text-lg font-medium text-zinc-100 mt-1">{value}</div>
              </div>
            ))}
          </div>

          {dexTotals && Object.keys(dexTotals).length > 0 && (
            <section>
              <h2 className="text-[15px] font-medium text-zinc-200 mb-3">DEX legs (all days)</h2>
              <div className="flex flex-wrap gap-2">
                {Object.entries(dexTotals)
                  .sort((a, b) => b[1] - a[1])
                  .map(([dex, n]) => (
                    <span
                      key={dex}
                      className="text-[12px] px-2.5 py-1 rounded-md bg-zinc-800/80 border border-white/[0.06] text-zinc-300"
                    >
                      {dex} <span className="text-zinc-500">{n}</span>
                    </span>
                  ))}
              </div>
            </section>
          )}

          <section>
            <h2 className="text-[15px] font-medium text-zinc-200 mb-3">Daily rollup</h2>
            <div className="overflow-x-auto rounded-xl border border-white/[0.08]">
              <table className="w-full text-[12px] text-left">
                <thead className="bg-zinc-900/60 text-zinc-500">
                  <tr>
                    <th className="px-3 py-2 font-medium">Day</th>
                    <th className="px-3 py-2 font-medium">Txs</th>
                    <th className="px-3 py-2 font-medium">Users</th>
                    <th className="px-3 py-2 font-medium">Vol in (XLM)</th>
                    <th className="px-3 py-2 font-medium">Split</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-white/[0.06]">
                  {[...data.daily].reverse().map((d) => (
                    <tr key={d.day} className="text-zinc-300">
                      <td className="px-3 py-2">{d.day}</td>
                      <td className="px-3 py-2">{d.tx_count}</td>
                      <td className="px-3 py-2">{d.unique_users}</td>
                      <td className="px-3 py-2">{stroopsToXlm(d.total_amount_in)}</td>
                      <td className="px-3 py-2">{d.split_swap_count}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <p className="text-[12px] text-zinc-600">
            API: <code className="text-zinc-500">{API_URL}/api/v1/stats</code>
            {' · '}
            <a
              href={`${API_URL}/api/v1/stats?format=csv`}
              className="text-zinc-500 hover:text-zinc-300 underline"
            >
              CSV export
            </a>
            {' · '}
            CLI: <code className="text-zinc-500">analytics-indexer export-daily</code>
          </p>
        </>
      )}
    </div>
  );
}

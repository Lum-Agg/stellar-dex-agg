'use client';

import { useCallback, useState, type CSSProperties } from 'react';

function shortAddress(address: string): string {
  return `${address.slice(0, 4)}…${address.slice(-4)}`;
}

function avatarStyle(address: string): CSSProperties {
  let hash = 0;
  for (let i = 0; i < address.length; i++) {
    hash = address.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  const hue2 = (hue + 48) % 360;
  return {
    background: `linear-gradient(135deg, hsl(${hue}, 42%, 38%), hsl(${hue2}, 48%, 28%))`,
  };
}

function formatUsd(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return value.toLocaleString(undefined, {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 2,
  });
}

export function ProfileHero({
  address,
  total,
  holdingCount,
  pricedHoldingCount,
  pricingLoading,
  balancesLoading,
  lastUpdatedAt,
  onRefresh,
}: {
  address: string;
  total: number | null;
  holdingCount: number;
  pricedHoldingCount: number;
  pricingLoading: boolean;
  balancesLoading: boolean;
  lastUpdatedAt: number | null;
  onRefresh: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const copyAddress = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(address);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  }, [address]);

  return (
    <section className="space-y-6 border-b border-[var(--border)] pb-8">
      <div className="flex items-start gap-4">
        <div
          className="flex h-14 w-14 shrink-0 items-center justify-center rounded-full text-[18px] font-semibold text-white shadow-inner"
          style={avatarStyle(address)}
          aria-hidden
        >
          {address.slice(0, 1).toUpperCase()}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h1 className="text-[20px] sm:text-[22px] font-semibold tracking-tight text-[var(--text-primary)]">
              {shortAddress(address)}
            </h1>
            <button
              type="button"
              onClick={() => void copyAddress()}
              className="shrink-0 rounded-lg border border-[var(--border)] px-2 py-1 text-[11px] text-[var(--text-muted)] hover:border-[var(--accent)]/40 hover:text-[var(--accent)] transition-colors"
              aria-label="Copy address"
            >
              {copied ? 'Copied' : 'Copy'}
            </button>
          </div>
          <p className="mt-1 truncate font-[family-name:var(--font-mono)] text-[12px] sm:text-[13px] text-[var(--text-muted)]">
            {address}
          </p>
        </div>
      </div>

      <div>
        <div className="mb-1 text-[11px] font-medium uppercase tracking-[0.14em] text-[var(--text-muted)]">
          {pricedHoldingCount < holdingCount
            ? 'Priced assets subtotal'
            : 'Estimated portfolio value'}
        </div>
        <div className="text-[36px] sm:text-[42px] font-semibold tracking-tight tabular-nums text-[var(--text-primary)]">
          {formatUsd(total)}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px] sm:text-[14px] text-[var(--text-muted)]">
          <span>
            {pricingLoading
              ? 'Updating prices…'
              : `${pricedHoldingCount} of ${holdingCount} assets priced via LumAgg quotes`}
          </span>
          {lastUpdatedAt && (
            <span className="tabular-nums">
              Balances updated{' '}
              {new Date(lastUpdatedAt).toLocaleTimeString([], {
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
              })}
            </span>
          )}
          <button
            type="button"
            onClick={onRefresh}
            disabled={balancesLoading}
            className="font-medium text-[var(--text-secondary)] hover:text-[var(--accent)] disabled:cursor-wait disabled:opacity-50 transition-colors"
          >
            {balancesLoading ? 'Refreshing…' : 'Refresh balances'}
          </button>
        </div>
      </div>
    </section>
  );
}

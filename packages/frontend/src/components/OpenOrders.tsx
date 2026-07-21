'use client';

import { useCallback, useEffect, useState } from 'react';
import { useWallet } from '@/lib/wallet-context';
import {
  TESTNET_TOKENS,
  e7ToPriceHuman,
  formatStroops,
  buildCancelOrder,
  isLimitApiConfigured,
  listOpenOrders,
  LIMIT_NETWORK_PASSPHRASE,
  submitLimitTx,
  tokenSymbol,
  type LimitOrder,
} from '@/lib/limit-orders';

export function OpenOrders({
  refreshKey = 0,
  onChanged,
}: {
  refreshKey?: number;
  onChanged?: () => void;
}) {
  const { address, signTx, connect, connecting } = useWallet();
  const [orders, setOrders] = useState<LimitOrder[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cancellingId, setCancellingId] = useState<number | null>(null);

  const configured = isLimitApiConfigured();

  const load = useCallback(async () => {
    if (!address || !configured) {
      setOrders([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const rows = await listOpenOrders(address);
      setOrders(rows);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to load orders');
      setOrders([]);
    } finally {
      setLoading(false);
    }
  }, [address, configured]);

  useEffect(() => {
    void load();
  }, [load, refreshKey]);

  const handleCancel = useCallback(
    async (orderId: number) => {
      if (!address) {
        connect();
        return;
      }
      setCancellingId(orderId);
      setError(null);
      try {
        const built = await buildCancelOrder({ user: address, orderId });
        if (!built.unsignedTxXdr) throw new Error('Empty unsigned XDR');
        const signed = await signTx(built.unsignedTxXdr, {
          networkPassphrase: LIMIT_NETWORK_PASSPHRASE,
        });
        await submitLimitTx(signed);
        setTimeout(() => {
          void load();
          onChanged?.();
        }, 2500);
        setTimeout(() => void load(), 8000);
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : 'Cancel failed');
      } finally {
        setCancellingId(null);
      }
    },
    [address, connect, signTx, load, onChanged],
  );

  if (!configured) return null;

  return (
    <div className="surface-panel p-5 sm:p-6">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-[15px] sm:text-[16px] font-semibold text-[var(--text-primary)]">
          Open orders
        </h3>
        {address && (
          <button
            type="button"
            onClick={() => void load()}
            disabled={loading}
            className="text-[13px] text-[var(--text-muted)] hover:text-[var(--accent)] disabled:opacity-40"
          >
            Refresh
          </button>
        )}
      </div>

      {!address ? (
        <p className="text-[13px] text-[var(--text-muted)]">
          <button
            type="button"
            onClick={connect}
            disabled={connecting}
            className="text-[var(--accent)] hover:underline"
          >
            Connect wallet
          </button>{' '}
          to see open limit orders.
        </p>
      ) : loading && orders.length === 0 ? (
        <p className="text-[13px] text-[var(--text-muted)]">Loading…</p>
      ) : orders.length === 0 ? (
        <p className="text-[13px] text-[var(--text-muted)]">No open orders.</p>
      ) : (
        <ul className="space-y-2">
          {orders.map((o) => {
            const inSym = tokenSymbol(o.tokenIn, TESTNET_TOKENS);
            const outSym = tokenSymbol(o.tokenOut, TESTNET_TOKENS);
            const remaining = formatStroops(o.amountInRemaining, 7);
            const price = e7ToPriceHuman(o.limitOutPerInE7, 7, 7);
            const busy = cancellingId === o.orderId;
            return (
              <li
                key={o.orderId}
                className="flex flex-col sm:flex-row sm:items-center gap-2 sm:gap-3 rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/40 px-3 py-3"
              >
                <div className="flex-1 min-w-0">
                  <p className="text-[14px] text-[var(--text-primary)] font-medium">
                    {remaining} {inSym} → {outSym}
                  </p>
                  <p className="text-[12px] text-[var(--text-muted)] mt-0.5 tabular-nums">
                    @ {price} {outSym}/{inSym} · exp ledger {o.expiresLedger} · #{o.orderId}
                  </p>
                </div>
                <button
                  type="button"
                  disabled={busy || cancellingId !== null}
                  onClick={() => void handleCancel(o.orderId)}
                  className="shrink-0 text-[13px] px-3 py-1.5 rounded-lg border border-[var(--border)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)] disabled:opacity-40"
                >
                  {busy ? 'Cancelling…' : 'Cancel'}
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {error && (
        <p className="mt-3 text-[13px] text-red-300/90 border border-red-500/15 bg-red-500/[0.05] rounded-xl px-3 py-2.5">
          {error}
        </p>
      )}
    </div>
  );
}

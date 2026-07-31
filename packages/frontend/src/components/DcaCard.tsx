'use client';

import { useEffect, useState } from 'react';
import { TokenSelector, type Token } from '@/components/TokenSelector';
import { useWallet } from '@/lib/wallet-context';
import {
  TESTNET_TOKENS,
  LIMIT_NETWORK_PASSPHRASE,
  amountToStroops,
  buildCancelDca,
  buildCreateDca,
  fetchLatestLedger,
  formatStroops,
  isLimitApiConfigured,
  listDcaOrders,
  priceHumanToE7,
  submitLimitTx,
  tokenSymbol,
  type DcaOrder,
} from '@/lib/limit-orders';

const INTERVALS = [
  { label: 'Hourly', ledgers: 720 },
  { label: 'Every 6h', ledgers: 4_320 },
  { label: 'Daily', ledgers: 17_280 },
] as const;
const MAX_LIFETIME_LEDGERS = 30 * 17_280;

export function DcaCard() {
  const { address, connect, signTx } = useWallet();
  const [tokenIn, setTokenIn] = useState<Token>(TESTNET_TOKENS[0] as Token);
  const [tokenOut, setTokenOut] = useState<Token>(TESTNET_TOKENS[1] as Token);
  const [total, setTotal] = useState('');
  const [chunk, setChunk] = useState('');
  const [interval, setInterval] = useState<number>(INTERVALS[2].ledgers);
  const [floor, setFloor] = useState('');
  const [orders, setOrders] = useState<DcaOrder[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const configured = isLimitApiConfigured();

  const refresh = async () => {
    if (!address || !configured) return;
    try {
      setOrders(await listDcaOrders(address));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load DCA orders');
    }
  };

  useEffect(() => {
    void refresh();
    // Refresh when the connected wallet changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [address]);

  const create = async () => {
    if (!address) return connect();
    setBusy(true);
    setError(null);
    try {
      const amountIn = amountToStroops(total, tokenIn.decimals);
      const chunkAmount = amountToStroops(chunk, tokenIn.decimals);
      const chunks = Math.ceil(Number(BigInt(amountIn)) / Number(BigInt(chunkAmount)));
      const duration = interval * Math.max(1, chunks);
      if (duration + 720 > MAX_LIFETIME_LEDGERS) {
        throw new Error('Schedule exceeds the 30-day testnet limit');
      }
      const latest = await fetchLatestLedger();
      const startLedger = latest + 12;
      const built = await buildCreateDca({
        user: address,
        tokenIn: tokenIn.id,
        tokenOut: tokenOut.id,
        amountIn,
        chunkAmount,
        intervalLedgers: interval,
        startLedger,
        minOutPerInE7: floor ? priceHumanToE7(floor, tokenIn.decimals, tokenOut.decimals) : '0',
        expiresLedger: startLedger + duration + 720,
      });
      const signed = await signTx(built.unsignedTxXdr, {
        networkPassphrase: LIMIT_NETWORK_PASSPHRASE,
      });
      await submitLimitTx(signed);
      setTotal('');
      setChunk('');
      setTimeout(() => void refresh(), 3_000);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create DCA order');
    } finally {
      setBusy(false);
    }
  };

  const cancel = async (orderId: number) => {
    if (!address) return;
    setBusy(true);
    setError(null);
    try {
      const built = await buildCancelDca({ user: address, orderId });
      const signed = await signTx(built.unsignedTxXdr, {
        networkPassphrase: LIMIT_NETWORK_PASSPHRASE,
      });
      await submitLimitTx(signed);
      setTimeout(() => void refresh(), 3_000);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to cancel DCA order');
    } finally {
      setBusy(false);
    }
  };

  const valid =
    configured && Number(total) > 0 && Number(chunk) > 0 && Number(chunk) <= Number(total);

  return (
    <div className="space-y-3">
      <div className="surface-panel p-5 sm:p-6">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-[18px] font-semibold">DCA</h2>
          <span className="rounded-lg border border-[var(--accent)]/35 px-2 py-1 text-[11px] uppercase text-[var(--accent)]">
            Testnet
          </span>
        </div>
        <p className="mb-4 text-[13px] text-[var(--text-muted)]">
          Lock a total amount and swap one chunk on each schedule. Keep your wallet on Stellar
          Testnet.
        </p>

        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="Total amount" value={total} onChange={setTotal}>
            <TokenSelector
              selected={tokenIn}
              tokens={TESTNET_TOKENS as Token[]}
              onSelect={setTokenIn}
              exclude={tokenOut.id}
            />
          </Field>
          <Field label="Amount per order" value={chunk} onChange={setChunk}>
            <span className="text-[14px] font-semibold">{tokenIn.symbol}</span>
          </Field>
        </div>

        <div className="mt-3 surface-panel-raised space-y-4 p-4">
          <div>
            <p className="mb-2 text-[13px] text-[var(--text-muted)]">Frequency</p>
            <div className="flex gap-1.5">
              {INTERVALS.map((item) => (
                <button
                  key={item.ledgers}
                  type="button"
                  onClick={() => setInterval(item.ledgers)}
                  className={`rounded-lg border px-3 py-1.5 text-[13px] ${interval === item.ledgers ? 'border-[var(--border-strong)] text-[var(--text-primary)]' : 'border-transparent text-[var(--text-muted)]'}`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>
          <label className="block text-[13px] text-[var(--text-muted)]">
            Minimum price per chunk <span className="text-[var(--text-muted)]/70">(optional)</span>
            <input
              value={floor}
              onChange={(e) => setFloor(e.target.value)}
              inputMode="decimal"
              placeholder={`1 ${tokenIn.symbol} = x ${tokenOut.symbol}`}
              className="mt-2 w-full rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/50 px-3 py-2.5 text-[var(--text-primary)] outline-none"
            />
          </label>
          <div className="flex items-center justify-between text-[13px] text-[var(--text-muted)]">
            <span>Receive</span>
            <TokenSelector
              selected={tokenOut}
              tokens={TESTNET_TOKENS as Token[]}
              onSelect={setTokenOut}
              exclude={tokenIn.id}
            />
          </div>
        </div>

        {error && <p className="mt-3 text-[13px] text-red-400">{error}</p>}
        <button
          type="button"
          disabled={!!address && (!valid || busy)}
          onClick={() => void create()}
          className="btn-primary mt-4 h-12 w-full disabled:opacity-50"
        >
          {!address
            ? 'Connect wallet'
            : busy
              ? 'Submitting...'
              : !configured
                ? 'API not configured'
                : 'Create DCA order'}
        </button>
      </div>

      {address && orders.length > 0 && (
        <div className="surface-panel p-5">
          <h3 className="mb-3 font-semibold">Open DCA orders</h3>
          {orders.map((order) => (
            <div
              key={order.orderId}
              className="flex items-center justify-between border-t border-[var(--border)] py-3 text-[13px]"
            >
              <div>
                <strong>
                  {tokenSymbol(order.tokenIn, TESTNET_TOKENS)} →{' '}
                  {tokenSymbol(order.tokenOut, TESTNET_TOKENS)}
                </strong>
                <p className="text-[var(--text-muted)]">
                  {formatStroops(order.amountInRemaining, 7)} remaining ·{' '}
                  {formatStroops(order.chunkAmount, 7)} per chunk · next ledger{' '}
                  {order.nextExecutableLedger.toLocaleString()}
                </p>
              </div>
              <button
                type="button"
                disabled={busy}
                onClick={() => void cancel(order.orderId)}
                className="text-red-400 hover:text-red-300"
              >
                Cancel
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: React.ReactNode;
}) {
  return (
    <label className="surface-panel-raised block p-4">
      <span className="text-[13px] text-[var(--text-muted)]">{label}</span>
      <div className="mt-2 flex items-center gap-2">
        <input
          value={value}
          onChange={(e) => /^\d*\.?\d*$/.test(e.target.value) && onChange(e.target.value)}
          inputMode="decimal"
          placeholder="0.0"
          className="min-w-0 flex-1 bg-transparent text-[25px] text-[var(--text-primary)] outline-none"
        />
        {children}
      </div>
    </label>
  );
}

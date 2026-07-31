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
    <div className="w-full max-w-none min-w-0 space-y-3">
      <div className="surface-panel p-5 sm:p-6 overflow-hidden">
        <div className="mb-4 flex items-center justify-between gap-3">
          <h2 className="text-[17px] sm:text-[18px] font-semibold tracking-tight text-[var(--text-primary)]">
            DCA
          </h2>
          <span className="shrink-0 rounded-lg border border-red-400/50 px-2 py-1 text-[11px] uppercase tracking-[0.06em] text-red-400">
            Testnet
          </span>
        </div>
        <p className="mb-4 text-[13px] leading-relaxed text-[var(--text-muted)]">
          Lock a total amount and swap one chunk on each schedule. Keep your wallet on Stellar
          Testnet.
        </p>

        <div className="surface-panel-raised p-4 sm:p-5">
          <div className="mb-2.5 text-[13px] sm:text-[14px] text-[var(--text-muted)]">Total amount</div>
          <div className="flex min-w-0 items-center gap-3">
            <input
              value={total}
              onChange={(e) => /^\d*\.?\d*$/.test(e.target.value) && setTotal(e.target.value)}
              inputMode="decimal"
              placeholder="0.0"
              className="min-w-0 flex-1 bg-transparent text-[25px] sm:text-[32px] font-medium tracking-tight text-[var(--text-primary)] outline-none placeholder-[var(--text-muted)]/50 font-[family-name:var(--font-mono)]"
            />
            <div className="shrink-0">
              <TokenSelector
                selected={tokenIn}
                tokens={TESTNET_TOKENS as Token[]}
                onSelect={setTokenIn}
                exclude={tokenOut.id}
              />
            </div>
          </div>
        </div>

        <div className="mt-3 surface-panel-raised p-4 sm:p-5">
          <div className="mb-2.5 text-[13px] sm:text-[14px] text-[var(--text-muted)]">
            Amount per order
          </div>
          <div className="flex min-w-0 items-center gap-3">
            <input
              value={chunk}
              onChange={(e) => /^\d*\.?\d*$/.test(e.target.value) && setChunk(e.target.value)}
              inputMode="decimal"
              placeholder="0.0"
              className="min-w-0 flex-1 bg-transparent text-[25px] sm:text-[32px] font-medium tracking-tight text-[var(--text-primary)] outline-none placeholder-[var(--text-muted)]/50 font-[family-name:var(--font-mono)]"
            />
            <span className="shrink-0 text-[14px] font-semibold text-[var(--text-primary)]">
              {tokenIn.symbol}
            </span>
          </div>
        </div>

        <div className="mt-3 space-y-4 overflow-hidden surface-panel-raised p-4 sm:p-5">
          <div className="min-w-0">
            <p className="mb-2 text-[13px] text-[var(--text-muted)]">Frequency</p>
            <div className="flex flex-wrap gap-1.5">
              {INTERVALS.map((item) => (
                <button
                  key={item.ledgers}
                  type="button"
                  onClick={() => setInterval(item.ledgers)}
                  className={`rounded-lg border px-3 py-1.5 text-[13px] whitespace-nowrap ${
                    interval === item.ledgers
                      ? 'border-[var(--border-strong)] text-[var(--text-primary)]'
                      : 'border-transparent text-[var(--text-muted)]'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>
          <label className="block min-w-0 text-[13px] text-[var(--text-muted)]">
            Minimum price per chunk <span className="text-[var(--text-muted)]/70">(optional)</span>
            <input
              value={floor}
              onChange={(e) => setFloor(e.target.value)}
              inputMode="decimal"
              placeholder={`1 ${tokenIn.symbol} = x ${tokenOut.symbol}`}
              className="mt-2 w-full min-w-0 rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/50 px-3 py-2.5 text-[var(--text-primary)] outline-none"
            />
          </label>
          <div className="flex min-w-0 items-center justify-between gap-3 text-[13px] text-[var(--text-muted)]">
            <span className="shrink-0">Receive</span>
            <div className="min-w-0 shrink">
              <TokenSelector
                selected={tokenOut}
                tokens={TESTNET_TOKENS as Token[]}
                onSelect={setTokenOut}
                exclude={tokenIn.id}
              />
            </div>
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
        <div className="surface-panel overflow-hidden p-5">
          <h3 className="mb-3 font-semibold">Open DCA orders</h3>
          {orders.map((order) => (
            <div
              key={order.orderId}
              className="flex min-w-0 items-start justify-between gap-3 border-t border-[var(--border)] py-3 text-[13px]"
            >
              <div className="min-w-0">
                <strong className="break-words">
                  {tokenSymbol(order.tokenIn, TESTNET_TOKENS)} →{' '}
                  {tokenSymbol(order.tokenOut, TESTNET_TOKENS)}
                </strong>
                <p className="break-words text-[var(--text-muted)]">
                  {formatStroops(order.amountInRemaining, 7)} remaining ·{' '}
                  {formatStroops(order.chunkAmount, 7)} per chunk · next ledger{' '}
                  {order.nextExecutableLedger.toLocaleString()}
                </p>
              </div>
              <button
                type="button"
                disabled={busy}
                onClick={() => void cancel(order.orderId)}
                className="shrink-0 text-red-400 hover:text-red-300"
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


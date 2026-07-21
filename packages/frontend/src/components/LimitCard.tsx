'use client';

import { useCallback, useState } from 'react';
import { TokenSelector, type Token } from '@/components/TokenSelector';
import { useWallet } from '@/lib/wallet-context';
import {
  TESTNET_TOKENS,
  EXPIRY_PRESETS,
  type ExpiryPresetId,
  amountToStroops,
  buildCreateOrder,
  fetchLatestLedger,
  isLimitApiConfigured,
  LIMIT_NETWORK_PASSPHRASE,
  priceHumanToE7,
  submitLimitTx,
} from '@/lib/limit-orders';
import { OpenOrders } from '@/components/OpenOrders';

export function LimitCard() {
  const { address: walletAddress, signTx, connect, connecting } = useWallet();
  const [tokenIn, setTokenIn] = useState<Token>(TESTNET_TOKENS[0] as Token);
  const [tokenOut, setTokenOut] = useState<Token>(TESTNET_TOKENS[1] as Token);
  const [amountIn, setAmountIn] = useState('');
  const [limitPrice, setLimitPrice] = useState('');
  const [expiry, setExpiry] = useState<ExpiryPresetId>('1d');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [txResult, setTxResult] = useState<{ success: boolean; hash?: string; error?: string } | null>(
    null,
  );
  const [refreshKey, setRefreshKey] = useState(0);

  const configured = isLimitApiConfigured();

  const swapDirection = useCallback(() => {
    setTokenIn(tokenOut);
    setTokenOut(tokenIn);
    setLimitPrice('');
  }, [tokenIn, tokenOut]);

  const handlePlace = useCallback(async () => {
    if (!walletAddress) {
      connect();
      return;
    }
    if (!configured) return;

    setSubmitting(true);
    setError(null);
    setTxResult(null);

    try {
      const amountStroops = amountToStroops(amountIn, tokenIn.decimals);
      const limitE7 = priceHumanToE7(limitPrice, tokenIn.decimals, tokenOut.decimals);
      const preset = EXPIRY_PRESETS.find((p) => p.id === expiry)!;
      const latest = await fetchLatestLedger();
      const expiresLedger = latest + preset.ledgers + 12; // small buffer

      const built = await buildCreateOrder({
        user: walletAddress,
        tokenIn: tokenIn.id,
        tokenOut: tokenOut.id,
        amountIn: amountStroops,
        limitOutPerInE7: limitE7,
        expiresLedger,
      });
      if (!built.unsignedTxXdr) throw new Error('Empty unsigned XDR');

      const signedXdr = await signTx(built.unsignedTxXdr, {
        networkPassphrase: LIMIT_NETWORK_PASSPHRASE,
      });
      const { hash } = await submitLimitTx(signedXdr);
      setTxResult({ success: true, hash });
      setAmountIn('');
      // Indexer may lag a few seconds
      setTimeout(() => setRefreshKey((k) => k + 1), 2500);
      setTimeout(() => setRefreshKey((k) => k + 1), 8000);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to place limit order';
      setError(msg);
      setTxResult({ success: false, error: msg });
    } finally {
      setSubmitting(false);
    }
  }, [
    walletAddress,
    connect,
    configured,
    amountIn,
    limitPrice,
    tokenIn,
    tokenOut,
    expiry,
    signTx,
  ]);

  const canSubmit =
    configured &&
    !!amountIn &&
    parseFloat(amountIn) > 0 &&
    !!limitPrice &&
    parseFloat(limitPrice) > 0 &&
    tokenIn.id !== tokenOut.id;

  const primaryDisabled =
    connecting || submitting || (walletAddress !== null && !canSubmit);

  const primaryLabel = connecting
    ? 'Connecting...'
    : submitting
      ? 'Submitting...'
      : !walletAddress
        ? 'Connect wallet'
        : !configured
          ? 'API not configured'
          : !amountIn
            ? 'Enter amount'
            : !limitPrice
              ? 'Enter limit price'
              : 'Place limit';

  return (
    <div className="w-full max-w-none space-y-3">
      <div className="surface-panel p-5 sm:p-6">
        <div className="flex items-center justify-between mb-4 gap-3">
          <h2 className="text-[17px] sm:text-[18px] font-semibold tracking-tight text-[var(--text-primary)]">
            Limit
          </h2>
          <span className="text-[11px] uppercase tracking-[0.06em] text-[var(--accent)] border border-[var(--accent)]/35 rounded-lg px-2 py-1">
            Testnet
          </span>
        </div>

        <p className="text-[13px] text-[var(--text-muted)] mb-4 leading-relaxed">
          Orders escrow on Stellar <span className="text-[var(--text-secondary)]">testnet</span>.
          Set Freighter (or your wallet) to Testnet before signing.
        </p>

        {!configured && (
          <div className="mb-4 text-[13px] text-[var(--text-secondary)] border border-[var(--border)] rounded-xl px-3 py-2.5">
            Testnet Limit API not configured. Set{' '}
            <code className="text-[12px] font-[family-name:var(--font-mono)] text-[var(--text-primary)]">
              NEXT_PUBLIC_LIMIT_API_URL
            </code>{' '}
            to enable create / cancel.
          </div>
        )}

        <div className="surface-panel-raised p-4 sm:p-5">
          <div className="flex justify-between items-center text-[13px] sm:text-[14px] text-[var(--text-muted)] mb-2.5">
            <span>Sell</span>
          </div>
          <div className="flex items-center gap-3">
            <input
              type="text"
              inputMode="decimal"
              value={amountIn}
              onChange={(e) => {
                const val = e.target.value;
                if (/^\d*\.?\d*$/.test(val)) setAmountIn(val);
              }}
              placeholder="0.0"
              disabled={!configured}
              className="flex-1 bg-transparent text-[32px] sm:text-[36px] font-medium tracking-tight outline-none placeholder-[var(--text-muted)]/50 min-w-0 text-[var(--text-primary)] font-[family-name:var(--font-mono)] disabled:opacity-50"
            />
            <TokenSelector
              selected={tokenIn}
              tokens={TESTNET_TOKENS as Token[]}
              onSelect={setTokenIn}
              exclude={tokenOut.id}
            />
          </div>
        </div>

        <div className="flex justify-center -my-2.5 relative z-10">
          <button
            type="button"
            onClick={swapDirection}
            disabled={!configured}
            className="w-10 h-10 rounded-xl bg-[var(--bg-0)] border border-[var(--border)] flex items-center justify-center hover:border-[var(--border-strong)] hover:bg-[var(--surface-raised)] transition-colors group disabled:opacity-50"
          >
            <svg
              className="w-4 h-4 text-[var(--text-muted)] group-hover:text-[var(--accent)] transition-colors"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
              />
            </svg>
          </button>
        </div>

        <div className="surface-panel-raised p-4 sm:p-5">
          <div className="flex justify-between text-[13px] sm:text-[14px] text-[var(--text-muted)] mb-2.5">
            <span>Buy</span>
          </div>
          <div className="flex items-center gap-3">
            <div className="flex-1 text-[32px] sm:text-[36px] font-medium tracking-tight min-w-0 font-[family-name:var(--font-mono)] text-[var(--text-muted)]/60">
              —
            </div>
            <TokenSelector
              selected={tokenOut}
              tokens={TESTNET_TOKENS as Token[]}
              onSelect={setTokenOut}
              exclude={tokenIn.id}
            />
          </div>
        </div>

        <div className="mt-3 surface-panel-raised p-4 sm:p-5 space-y-4">
          <div>
            <label className="block text-[13px] text-[var(--text-muted)] mb-2">
              Limit price{' '}
              <span className="text-[var(--text-secondary)]">
                (1 {tokenIn.symbol} = x {tokenOut.symbol})
              </span>
            </label>
            <input
              type="text"
              inputMode="decimal"
              value={limitPrice}
              onChange={(e) => {
                const val = e.target.value;
                if (/^\d*\.?\d*$/.test(val)) setLimitPrice(val);
              }}
              placeholder="0.0"
              disabled={!configured}
              className="w-full bg-[var(--bg-0)]/50 border border-[var(--border)] rounded-xl px-3 py-2.5 text-[16px] font-[family-name:var(--font-mono)] text-[var(--text-primary)] outline-none focus:border-[var(--border-strong)] disabled:opacity-50"
            />
          </div>

          <div>
            <p className="text-[13px] text-[var(--text-muted)] mb-2">Expires</p>
            <div className="flex items-center gap-1.5">
              {EXPIRY_PRESETS.map((p) => {
                const active = expiry === p.id;
                return (
                  <button
                    key={p.id}
                    type="button"
                    disabled={!configured}
                    onClick={() => setExpiry(p.id)}
                    className={`px-3 py-1.5 rounded-lg text-[13px] transition-colors border disabled:opacity-50 ${
                      active
                        ? 'bg-[var(--surface-raised)] text-[var(--text-primary)] border-[var(--border)]'
                        : 'text-[var(--text-muted)] border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border)]'
                    }`}
                  >
                    {p.label}
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {error && !submitting && (
          <div className="mt-3 text-[13px] text-red-300/90 border border-red-500/15 bg-red-500/[0.05] rounded-xl px-3 py-2.5 text-center">
            {error}
          </div>
        )}

        <div className="mt-5">
          <button
            type="button"
            onClick={() => void handlePlace()}
            disabled={primaryDisabled}
            className="btn-primary w-full py-4 text-[16px] sm:text-[17px]"
          >
            {primaryLabel}
          </button>
        </div>

        {txResult && (
          <div
            className={`mt-3 p-3 rounded-xl text-[13px] border ${
              txResult.success
                ? 'bg-emerald-500/[0.06] border-emerald-500/20 text-emerald-300'
                : 'bg-red-500/[0.05] border-red-500/15 text-red-300'
            }`}
          >
            {txResult.success ? (
              <div>
                Limit order submitted.{' '}
                <a
                  href={`https://stellar.expert/explorer/testnet/tx/${txResult.hash}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline"
                >
                  View transaction
                </a>
              </div>
            ) : (
              <div>{txResult.error}</div>
            )}
          </div>
        )}
      </div>

      <OpenOrders refreshKey={refreshKey} onChanged={() => setRefreshKey((k) => k + 1)} />
    </div>
  );
}

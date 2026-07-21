'use client';

import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { getQuote, type QuoteData } from '@/lib/aggregator';
import { formatBalanceDisplay, percentToAmountInput } from '@/lib/balance';
import { useAccountBalances } from '@/lib/account-balances-context';
import { useWallet } from '@/lib/wallet-context';
import { RouteDisplay } from './RouteDisplay';
import { TokenSelector, type Token, TOKENS, useTokenList } from './TokenSelector';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { SWAP_SUCCESS_EVENT } from '@/lib/swaps';
import {
  getSubmitViaPreference,
  setSubmitViaPreference,
  type SubmitVia,
} from '@/lib/rpc';
import { submitTransaction } from '@/lib/wallet';
import {
  buildChangeTrustXdr,
  canAddTrustlineForSac,
  classicAssetForSac,
} from '@/lib/trustline';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';
const SLIPPAGE_PRESETS = [0.1, 0.5, 1.0] as const;
const SLIPPAGE_MIN = 0.01;
const SLIPPAGE_MAX = 50;

function parseSlippageInput(raw: string): number | null {
  if (!raw || raw === '.') return null;
  const n = Number(raw);
  if (!Number.isFinite(n)) return null;
  if (n < SLIPPAGE_MIN || n > SLIPPAGE_MAX) return null;
  return n;
}

export function SwapCard() {
  const [tokenIn, setTokenIn] = useState<Token>(TOKENS[0]);
  const [tokenOut, setTokenOut] = useState<Token>(TOKENS[1]);
  const [amountIn, setAmountIn] = useState('');
  const [slippage, setSlippage] = useState(1.0);
  const [customSlippage, setCustomSlippage] = useState('');
  const [slippageCustomMode, setSlippageCustomMode] = useState(false);
  const [quote, setQuote] = useState<QuoteData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { address: walletAddress, signTx, connect, connecting } = useWallet();
  const {
    getBalance,
    getHasTrustline,
    ensureBalance,
    loading: balancesLoading,
    ready: balancesReady,
    refresh: refreshBalances,
  } = useAccountBalances();
  const debounceRef = useRef<NodeJS.Timeout | null>(null);
  const quoteFingerprintRef = useRef('');
  const tokenList = useTokenList();
  const quoteFingerprint = `${tokenIn.id}:${tokenOut.id}:${amountIn}:${slippage}`;
  quoteFingerprintRef.current = quoteFingerprint;
  const resolveTokenSymbol = useMemo(() => {
    const byId = new Map(tokenList.map((t) => [t.id, t.symbol]));
    return (contractId: string) => {
      const sym = byId.get(contractId);
      if (sym) return displayTokenSymbol(sym, contractId);
      if (contractId === NATIVE_CONTRACT || contractId === 'native') return 'XLM';
      return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
    };
  }, [tokenList]);

  const loadQuote = useCallback(
    async (opts?: { silent?: boolean }) => {
      const silent = opts?.silent ?? false;
      const requestFingerprint = `${tokenIn.id}:${tokenOut.id}:${amountIn}:${slippage}`;

      if (!amountIn || parseFloat(amountIn) <= 0) {
        setQuote(null);
        return;
      }

      if (!silent) {
        setLoading(true);
        setError(null);
      }

      try {
        const amountStroops = Math.floor(parseFloat(amountIn) * 10 ** tokenIn.decimals).toString();
        const result = await getQuote(tokenIn.id, tokenOut.id, amountStroops, slippage);

        if (requestFingerprint !== quoteFingerprintRef.current) return;

        if (result.success && result.data) {
          setQuote(result.data);
          setError(null);
        } else if (!silent) {
          setQuote(null);
          setError(result.error || 'No route found');
        }
      } catch {
        if (!silent && requestFingerprint === quoteFingerprintRef.current) {
          setQuote(null);
          setError('Failed to fetch quote');
        }
      } finally {
        if (!silent && requestFingerprint === quoteFingerprintRef.current) {
          setLoading(false);
        }
      }
    },
    [amountIn, tokenIn, tokenOut, slippage],
  );

  // Auto-fetch quote when amount changes (debounced)
  useEffect(() => {
    if (!amountIn || parseFloat(amountIn) <= 0) {
      setQuote(null);
      return;
    }

    if (debounceRef.current) clearTimeout(debounceRef.current);

    debounceRef.current = setTimeout(() => {
      void loadQuote();
    }, 500);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [amountIn, loadQuote]);

  useEffect(() => {
    if (!amountIn || parseFloat(amountIn) <= 0) return;

    const interval = setInterval(() => {
      void loadQuote({ silent: true });
    }, 12_000);

    return () => clearInterval(interval);
  }, [amountIn, loadQuote]);

  const swapDirection = () => {
    setTokenIn(tokenOut);
    setTokenOut(tokenIn);
    setQuote(null);
    setAmountIn('');
  };

  const formatOutput = (stroops: string) => {
    const val = parseInt(stroops) / 10 ** tokenOut.decimals;
    if (val >= 1000) return val.toFixed(2);
    if (val >= 1) return val.toFixed(4);
    return val.toFixed(7);
  };

  const [swapping, setSwapping] = useState(false);
  const [addingTrustline, setAddingTrustline] = useState(false);
  const [submitVia, setSubmitVia] = useState<SubmitVia>('lumagg');

  useEffect(() => {
    setSubmitVia(getSubmitViaPreference());
  }, []);
  const [txResult, setTxResult] = useState<{
    success: boolean;
    hash?: string;
    error?: string;
  } | null>(null);

  const balanceStroops = walletAddress ? getBalance(tokenIn.id) : null;
  const outputHasTrustline = walletAddress ? getHasTrustline(tokenOut.id) : null;

  useEffect(() => {
    if (!walletAddress || !balancesReady) return;
    void ensureBalance(tokenIn.id);
  }, [walletAddress, balancesReady, tokenIn.id, ensureBalance]);

  useEffect(() => {
    if (!walletAddress || !balancesReady) return;
    void ensureBalance(tokenOut.id);
  }, [walletAddress, balancesReady, tokenOut.id, ensureBalance]);

  const applyBalancePercent = useCallback(
    (percent: number) => {
      if (balanceStroops === null || balanceStroops === BigInt(0)) return;
      setAmountIn(percentToAmountInput(balanceStroops, percent, tokenIn.decimals, tokenIn.id));
      setQuote(null);
      setTxResult(null);
    },
    [balanceStroops, tokenIn.decimals, tokenIn.id],
  );

  const handleSwap = useCallback(async () => {
    if (!walletAddress || !quote) return;
    if (!quote.sub_routes?.length) {
      setTxResult({ success: false, error: 'No route to execute' });
      return;
    }
    setSwapping(true);
    setTxResult(null);

    try {
      const totalAmountIn = Math.floor(parseFloat(amountIn) * 10 ** tokenIn.decimals).toString();

      const subSum = quote.sub_routes.reduce((s, r) => s + BigInt(r.amount_in || '0'), BigInt(0));
      if (subSum.toString() !== totalAmountIn) {
        setTxResult({
          success: false,
          error:
            'Quote is out of date for this amount. Wait for the route to refresh, then try again.',
        });
        return;
      }
      if (quote.amount_in && quote.amount_in !== totalAmountIn) {
        setTxResult({
          success: false,
          error:
            'Quote is out of date for this amount. Wait for the route to refresh, then try again.',
        });
        return;
      }

      const cached = getBalance(tokenIn.id);
      const balance = cached ?? (await ensureBalance(tokenIn.id)) ?? BigInt(0);
      if (BigInt(totalAmountIn) > balance) {
        const have = Number(balance) / 10 ** tokenIn.decimals;
        const need = Number(totalAmountIn) / 10 ** tokenIn.decimals;
        setTxResult({
          success: false,
          error: `Insufficient ${tokenIn.symbol} balance: you have ~${have.toFixed(4)}, but this swap needs ~${need.toFixed(4)}.`,
        });
        return;
      }

      const sub_routes = quote.sub_routes.map((route) => ({
        amount_in: route.amount_in,
        steps: route.pool_addresses.map((pool: string, i: number) => ({
          dex_type: route.dex_types[i] ?? 'aquarius',
          pool_address: pool,
          token_in: route.path[i] ?? '',
          token_out: route.path[i + 1] ?? '',
          in_idx: route.in_indices[i] ?? 0,
          out_idx: route.out_indices[i] ?? 1,
        })),
      }));

      const buildResp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user_public_key: walletAddress,
          token_in: tokenIn.id,
          token_out: tokenOut.id,
          amount_in: totalAmountIn,
          min_amount_out: quote.minimum_output,
          sub_routes,
        }),
      });
      const buildData = await buildResp.json();

      if (!buildData.success || !buildData.data?.unsigned_tx_xdr) {
        setTxResult({ success: false, error: buildData.error || 'Failed to build transaction' });
        return;
      }

      // 2. Sign with wallet
      const signedXdr = await signTx(buildData.data.unsigned_tx_xdr);

      // 3. Submit (api-server by default, or official RPC if Advanced is on)
      const submitResult = await submitTransaction(signedXdr, { via: submitVia });

      if (submitResult.success) {
        setTxResult({ success: true, hash: submitResult.hash });
        window.dispatchEvent(new Event(SWAP_SUCCESS_EVENT));
        setAmountIn('');
        setQuote(null);
        void refreshBalances();
      } else {
        setTxResult({ success: false, error: submitResult.error || 'Transaction failed' });
      }
    } catch (err: any) {
      setTxResult({ success: false, error: err.message || 'Swap failed' });
    } finally {
      setSwapping(false);
    }
  }, [
    walletAddress,
    quote,
    tokenIn,
    tokenOut,
    amountIn,
    signTx,
    refreshBalances,
    getBalance,
    ensureBalance,
    submitVia,
  ]);

  const handleAddTrustline = useCallback(async () => {
    if (!walletAddress) return;
    const asset = classicAssetForSac(tokenOut.id);
    if (!asset) {
      setTxResult({
        success: false,
        error: `Add a ${tokenOut.symbol} trustline manually in your wallet, then refresh.`,
      });
      return;
    }

    setAddingTrustline(true);
    setTxResult(null);
    try {
      const unsignedXdr = await buildChangeTrustXdr(walletAddress, asset);
      const signedXdr = await signTx(unsignedXdr);
      const result = await submitTransaction(signedXdr, { via: submitVia });
      if (result.success) {
        setTxResult({ success: true, hash: result.hash });
        void refreshBalances();
        void ensureBalance(tokenOut.id);
      } else {
        setTxResult({ success: false, error: result.error || 'Failed to add trustline' });
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : 'Failed to add trustline';
      setTxResult({ success: false, error: message });
    } finally {
      setAddingTrustline(false);
    }
  }, [
    walletAddress,
    tokenOut.id,
    tokenOut.symbol,
    signTx,
    refreshBalances,
    ensureBalance,
    submitVia,
  ]);

  const handlePrimaryAction = useCallback(() => {
    if (!walletAddress) {
      connect();
      return;
    }
    if (outputHasTrustline === false) {
      void handleAddTrustline();
      return;
    }
    handleSwap();
  }, [walletAddress, connect, handleSwap, outputHasTrustline, handleAddTrustline]);

  const needsTrustline = walletAddress !== null && outputHasTrustline === false;
  const canAutoAddTrustline = needsTrustline && canAddTrustlineForSac(tokenOut.id);

  const primaryDisabled =
    connecting ||
    swapping ||
    addingTrustline ||
    (needsTrustline && !canAutoAddTrustline) ||
    (walletAddress !== null && !needsTrustline && (loading || !quote || !amountIn));

  const primaryLabel = connecting
    ? 'Connecting...'
    : addingTrustline
      ? 'Adding trustline...'
      : swapping
        ? 'Submitting...'
        : loading && walletAddress && !needsTrustline
          ? 'Finding best route...'
          : !walletAddress
            ? 'Connect wallet to swap'
            : needsTrustline
              ? canAutoAddTrustline
                ? `Add ${tokenOut.symbol} trustline`
                : `Add ${tokenOut.symbol} trustline in wallet`
              : !amountIn
                ? 'Enter amount'
                : !quote
                  ? 'No route available'
                  : 'Review & swap';

  return (
    <div className="w-full max-w-none space-y-3">
      <div className="surface-panel p-5 sm:p-6">
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-[17px] sm:text-[18px] font-semibold tracking-tight text-[var(--text-primary)]">
            Swap
          </h2>
          <div className="flex items-center gap-0.5 rounded-full border border-[var(--border)] bg-[var(--bg-0)]/50 p-0.5">
            {SLIPPAGE_PRESETS.map((s) => {
              const active = !slippageCustomMode && slippage === s;
              return (
                <button
                  key={s}
                  type="button"
                  onClick={() => {
                    setSlippage(s);
                    setSlippageCustomMode(false);
                    setCustomSlippage('');
                  }}
                  className={`px-3 py-1.5 rounded-full text-[13px] transition-colors ${
                    active
                      ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
                      : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
                  }`}
                >
                  {s}%
                </button>
              );
            })}
            <label
              className={`flex items-center gap-0.5 rounded-full px-2 py-1 transition-colors ${
                slippageCustomMode
                  ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
                  : 'text-[var(--text-muted)]'
              }`}
            >
              <input
                type="text"
                inputMode="decimal"
                aria-label="Custom slippage percent"
                placeholder="Custom"
                value={slippageCustomMode ? customSlippage : ''}
                onFocus={() => {
                  setSlippageCustomMode(true);
                  setCustomSlippage(customSlippage || String(slippage));
                }}
                onChange={(e) => {
                  const val = e.target.value;
                  if (!/^\d*\.?\d*$/.test(val)) return;
                  setSlippageCustomMode(true);
                  setCustomSlippage(val);
                  const parsed = parseSlippageInput(val);
                  if (parsed !== null) setSlippage(parsed);
                }}
                onBlur={() => {
                  const parsed = parseSlippageInput(customSlippage);
                  if (parsed === null) {
                    // Keep last valid slippage; snap UI back to a preset if it matches.
                    const preset = SLIPPAGE_PRESETS.find((s) => s === slippage);
                    if (preset !== undefined) {
                      setSlippageCustomMode(false);
                      setCustomSlippage('');
                    } else {
                      setCustomSlippage(String(slippage));
                    }
                    return;
                  }
                  setSlippage(parsed);
                  setCustomSlippage(String(parsed));
                }}
                className="w-12 bg-transparent text-[13px] text-right outline-none placeholder-[var(--text-muted)]/70 tabular-nums"
              />
              <span className="text-[13px] shrink-0">%</span>
            </label>
          </div>
        </div>

        <div className="surface-panel-raised p-4 sm:p-5">
          <div className="flex justify-between items-center text-[13px] sm:text-[14px] text-[var(--text-muted)] mb-2.5 gap-2">
            <span>Sell</span>
            {walletAddress && (
              <span className="text-[var(--text-secondary)] truncate tabular-nums font-[family-name:var(--font-mono)] text-[13px]">
                {balancesLoading && !balancesReady ? (
                  'Balance…'
                ) : balanceStroops !== null ? (
                  <>
                    {formatBalanceDisplay(balanceStroops, tokenIn.decimals)} {tokenIn.symbol}
                  </>
                ) : (
                  '—'
                )}
              </span>
            )}
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
              className="flex-1 bg-transparent text-[32px] sm:text-[36px] font-medium tracking-tight outline-none placeholder-[var(--text-muted)]/50 min-w-0 text-[var(--text-primary)] font-[family-name:var(--font-mono)]"
            />
            <TokenSelector
              selected={tokenIn}
              onSelect={(t) => {
                setTokenIn(t);
                setQuote(null);
                setAmountIn('');
              }}
              exclude={tokenOut.id}
            />
          </div>
          {walletAddress &&
            balancesReady &&
            balanceStroops !== null &&
            balanceStroops > BigInt(0) && (
              <div className="flex items-center gap-1.5 mt-3">
                {[25, 50, 75, 100].map((pct) => (
                  <button
                    key={pct}
                    type="button"
                    onClick={() => applyBalancePercent(pct)}
                    className="px-2.5 py-1 rounded-lg text-[13px] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-0)] border border-transparent hover:border-[var(--border)] transition-colors"
                  >
                    {pct === 100 ? 'Max' : `${pct}%`}
                  </button>
                ))}
              </div>
            )}
        </div>

        <div className="flex justify-center -my-2.5 relative z-10">
          <button
            onClick={swapDirection}
            className="w-10 h-10 rounded-xl bg-[var(--bg-0)] border border-[var(--border)] flex items-center justify-center hover:border-[var(--border-strong)] hover:bg-[var(--surface-raised)] transition-colors group"
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
            {loading && <span className="text-[var(--accent)]/80">Finding route…</span>}
          </div>
          <div className="flex items-center gap-3">
            <div className="flex-1 text-[32px] sm:text-[36px] font-medium tracking-tight min-w-0 font-[family-name:var(--font-mono)]">
              {quote ? (
                <span className="text-[var(--text-primary)]">
                  {formatOutput(quote.expected_output)}
                </span>
              ) : (
                <span className="text-[var(--text-muted)]/60">0.0</span>
              )}
            </div>
            <TokenSelector
              selected={tokenOut}
              onSelect={(t) => {
                setTokenOut(t);
                setQuote(null);
              }}
              exclude={tokenIn.id}
            />
          </div>
        </div>

        {quote && amountIn && parseFloat(amountIn) > 0 && (
          <div className="mt-3.5 px-0.5 flex items-center justify-between gap-3 text-[13px] sm:text-[14px] text-[var(--text-muted)]">
            <span className="tabular-nums">
              1 {tokenIn.symbol} ≈{' '}
              {(
                parseInt(quote.expected_output, 10) /
                10 ** tokenOut.decimals /
                parseFloat(amountIn)
              ).toLocaleString(undefined, {
                maximumFractionDigits: 8,
              })}{' '}
              {tokenOut.symbol}
            </span>
            <button
              type="button"
              onClick={() => void loadQuote({ silent: true })}
              disabled={loading}
              className="shrink-0 text-[var(--text-secondary)] hover:text-[var(--accent)] disabled:opacity-40 transition-colors"
            >
              Refresh
            </button>
          </div>
        )}

        {error && !loading && (
          <div className="mt-3 text-[13px] text-red-300/90 border border-red-500/15 bg-red-500/[0.05] rounded-xl px-3 py-2.5 text-center">
            {error === 'Failed to fetch quote'
              ? 'Unable to load quote. Please retry in a moment.'
              : error}
          </div>
        )}

        {walletAddress && outputHasTrustline === false && (
          <div className="mt-3 text-[13px] text-amber-200/90 border border-amber-500/20 bg-amber-500/[0.06] rounded-xl px-3 py-2.5 text-center">
            {canAutoAddTrustline
              ? `Your wallet cannot receive ${tokenOut.symbol} yet. Add a trustline (~0.5 XLM reserve), then swap.`
              : `Add a ${tokenOut.symbol} trustline in your wallet before swapping (~0.5 XLM reserve).`}
          </div>
        )}

        <div className="mt-5">
          <button
            type="button"
            onClick={handlePrimaryAction}
            disabled={primaryDisabled}
            className="btn-primary w-full py-4 text-[16px] sm:text-[17px]"
          >
            {primaryLabel}
          </button>
        </div>

        {txResult && (
          <div
            className={`mt-3 p-3 rounded-xl text-[13px] border ${txResult.success ? 'bg-emerald-500/[0.06] border-emerald-500/20 text-emerald-300' : 'bg-red-500/[0.05] border-red-500/15 text-red-300'}`}
          >
            {txResult.success ? (
              <div>
                {needsTrustline || addingTrustline
                  ? 'Trustline added. '
                  : 'Swap submitted successfully. '}
                <a
                  href={`https://stellar.expert/explorer/public/tx/${txResult.hash}`}
                  target="_blank"
                  rel="noopener"
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

        <label className="mt-4 flex items-start gap-2 cursor-pointer select-none text-[11px] leading-snug text-[var(--text-muted)]/70 hover:text-[var(--text-muted)]">
          <input
            type="checkbox"
            className="mt-0.5 accent-[var(--accent)]"
            checked={submitVia === 'official'}
            onChange={(e) => {
              const next: SubmitVia = e.target.checked ? 'official' : 'lumagg';
              setSubmitVia(next);
              setSubmitViaPreference(next);
            }}
          />
          <span>
            Advanced: submit via official RPC
            {submitVia === 'official' ? ' (mainnet.sorobanrpc.com)' : ''}
          </span>
        </label>
      </div>

      {/* Route Details */}
      {quote && (
        <RouteDisplay
          quote={quote}
          tokenInSymbol={tokenIn.symbol}
          tokenOutSymbol={tokenOut.symbol}
          tokenInDecimals={tokenIn.decimals}
          tokenOutDecimals={tokenOut.decimals}
          resolveTokenSymbol={resolveTokenSymbol}
        />
      )}
    </div>
  );
}

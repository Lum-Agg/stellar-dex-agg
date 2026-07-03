'use client';

import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { getQuote, type QuoteData } from '@/lib/aggregator';
import { fetchSpendableBalanceStroops } from '@/lib/balance';
import { useWallet } from '@/lib/wallet-context';
import { RouteDisplay } from './RouteDisplay';
import { TokenSelector, type Token, TOKENS, useTokenList } from './TokenSelector';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export function SwapCard() {
  const [tokenIn, setTokenIn] = useState<Token>(TOKENS[0]);
  const [tokenOut, setTokenOut] = useState<Token>(TOKENS[1]);
  const [amountIn, setAmountIn] = useState('');
  const [slippage, setSlippage] = useState(1.0);
  const [quote, setQuote] = useState<QuoteData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { address: walletAddress, signTx, connect, connecting } = useWallet();
  const debounceRef = useRef<NodeJS.Timeout | null>(null);
  const tokenList = useTokenList();
  const resolveTokenSymbol = useMemo(() => {
    const byId = new Map(tokenList.map((t) => [t.id, t.symbol]));
    return (contractId: string) => {
      const sym = byId.get(contractId);
      if (sym) return displayTokenSymbol(sym, contractId);
      if (contractId === NATIVE_CONTRACT || contractId === 'native') return 'XLM';
      return `${contractId.slice(0, 4)}…${contractId.slice(-4)}`;
    };
  }, [tokenList]);

  // Auto-fetch quote when amount changes (debounced)
  useEffect(() => {
    if (!amountIn || parseFloat(amountIn) <= 0) {
      setQuote(null);
      return;
    }

    if (debounceRef.current) clearTimeout(debounceRef.current);

    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      setError(null);

      try {
        const amountStroops = Math.floor(parseFloat(amountIn) * 10 ** tokenIn.decimals).toString();
        const result = await getQuote(tokenIn.id, tokenOut.id, amountStroops, slippage);

        if (result.success && result.data) {
          setQuote(result.data);
          setError(null);
        } else {
          setQuote(null);
          setError(result.error || 'No route found');
        }
      } catch {
        setQuote(null);
        setError('Failed to fetch quote');
      } finally {
        setLoading(false);
      }
    }, 500);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [amountIn, tokenIn, tokenOut, slippage]);

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
  const [txResult, setTxResult] = useState<{ success: boolean; hash?: string; error?: string } | null>(null);

  const handleSwap = useCallback(async () => {
    if (!walletAddress || !quote) return;
    if (!quote.sub_routes?.length) {
      setTxResult({ success: false, error: 'No route to execute' });
      return;
    }
    setSwapping(true);
    setTxResult(null);

    try {
      const totalAmountIn = Math.floor(
        parseFloat(amountIn) * 10 ** tokenIn.decimals
      ).toString();

      const subSum = quote.sub_routes.reduce(
        (s, r) => s + BigInt(r.amount_in || '0'),
        BigInt(0)
      );
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

      const balance = await fetchSpendableBalanceStroops(
        walletAddress,
        tokenIn.id,
        tokenIn.decimals
      );
      if (balance !== null && BigInt(totalAmountIn) > balance) {
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

      // 3. Submit to Horizon
      const submitResp = await fetch('https://horizon.stellar.org/transactions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: `tx=${encodeURIComponent(signedXdr)}`,
      });
      const submitData = await submitResp.json();

      if (submitData.successful || submitData.hash) {
        setTxResult({ success: true, hash: submitData.hash });
        setAmountIn('');
        setQuote(null);
      } else {
        const errMsg = submitData.extras?.result_codes?.operations?.[0] || submitData.title || 'Transaction failed';
        setTxResult({ success: false, error: errMsg });
      }
    } catch (err: any) {
      setTxResult({ success: false, error: err.message || 'Swap failed' });
    } finally {
      setSwapping(false);
    }
  }, [walletAddress, quote, tokenIn, tokenOut, amountIn, signTx]);

  const handlePrimaryAction = useCallback(() => {
    if (!walletAddress) {
      connect();
      return;
    }
    handleSwap();
  }, [walletAddress, connect, handleSwap]);


  const primaryDisabled =
    connecting ||
    swapping ||
    (walletAddress !== null && (loading || !quote || !amountIn));

  const primaryLabel = connecting
    ? 'Connecting...'
    : swapping
      ? 'Submitting...'
      : loading && walletAddress
        ? 'Finding best route...'
        : !walletAddress
          ? 'Connect wallet to swap'
          : !amountIn
            ? 'Enter amount'
            : !quote
              ? 'No route available'
              : 'Review & swap';

  return (
    <div className="w-full max-w-[420px] space-y-3">
      <div className="surface-panel p-5">
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="text-[15px] font-semibold text-zinc-100">Swap</h2>
            <p className="text-[12px] text-zinc-500 mt-0.5">Quotes include routing + slippage protection</p>
          </div>
          <div className="flex items-center gap-1">
            {[0.1, 0.5, 1.0].map(s => (
              <button
                key={s}
                onClick={() => setSlippage(s)}
                className={`px-2 py-1 rounded-md text-[12px] transition-colors ${
                  slippage === s
                    ? 'bg-zinc-800 text-zinc-100 border border-white/[0.1]'
                    : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                {s}%
              </button>
            ))}
          </div>
        </div>

        <div className="surface-panel-raised p-4">
          <div className="flex justify-between text-[12px] text-zinc-500 mb-2">
            <span>You pay</span>
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
              placeholder="0"
              className="flex-1 bg-transparent text-2xl font-medium tracking-tight outline-none placeholder-zinc-600 min-w-0 text-zinc-50"
            />
            <TokenSelector
              selected={tokenIn}
              onSelect={(t) => { setTokenIn(t); setQuote(null); }}
              exclude={tokenOut.id}
            />
          </div>
        </div>

        <div className="flex justify-center -my-2.5 relative z-10">
          <button
            onClick={swapDirection}
            className="w-8 h-8 rounded-lg bg-[#141419] border border-white/[0.1] flex items-center justify-center hover:bg-zinc-800 hover:border-white/[0.15] transition-colors group"
          >
            <svg className="w-3.5 h-3.5 text-zinc-500 group-hover:text-zinc-300 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
            </svg>
          </button>
        </div>

        <div className="surface-panel-raised p-4">
          <div className="flex justify-between text-[12px] text-zinc-500 mb-2">
            <span>You receive</span>
            {loading && (
              <span className="text-zinc-400">Finding best route...</span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <div className="flex-1 text-2xl font-medium tracking-tight min-w-0">
              {quote ? (
                <span className="text-zinc-50">{formatOutput(quote.expected_output)}</span>
              ) : (
                <span className="text-zinc-600">0</span>
              )}
            </div>
            <TokenSelector
              selected={tokenOut}
              onSelect={(t) => { setTokenOut(t); setQuote(null); }}
              exclude={tokenIn.id}
            />
          </div>
        </div>

        {/* Rate display: human out per 1 unit token in (not stroops passed to formatOutput) */}
        {quote && amountIn && parseFloat(amountIn) > 0 && (
          <div className="mt-3 px-0.5 text-[12px] text-zinc-500">
            1 {tokenIn.symbol} ≈{' '}
            {(parseInt(quote.expected_output, 10) / 10 ** tokenOut.decimals / parseFloat(amountIn)).toLocaleString(undefined, {
              maximumFractionDigits: 8,
            })}{' '}
            {tokenOut.symbol}
          </div>
        )}

        {/* Error */}
        {error && !loading && (
          <div className="mt-3 text-[12px] text-red-300 border border-red-500/20 bg-red-500/[0.06] rounded-lg px-3 py-2 text-center">
            {error === 'Failed to fetch quote'
              ? 'Unable to load quote. Please retry in a moment.'
              : error}
          </div>
        )}

        <div className="mt-5">
          <button
            type="button"
            onClick={handlePrimaryAction}
            disabled={primaryDisabled}
            className="btn-primary w-full py-3 text-[15px]"
          >
            {primaryLabel}
          </button>
        </div>

        {txResult && (
          <div className={`mt-3 p-3 rounded-lg text-[12px] border ${txResult.success ? 'bg-emerald-500/[0.06] border-emerald-500/20 text-emerald-300' : 'bg-red-500/[0.06] border-red-500/20 text-red-300'}`}>
            {txResult.success ? (
              <div>
                Swap submitted successfully.{' '}
                <a href={`https://stellar.expert/explorer/public/tx/${txResult.hash}`} target="_blank" rel="noopener" className="underline">
                  View transaction
                </a>
              </div>
            ) : (
              <div>{txResult.error}</div>
            )}
          </div>
        )}
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

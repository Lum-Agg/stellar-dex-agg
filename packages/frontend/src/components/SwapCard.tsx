'use client';

import { useState, useCallback, useEffect, useRef } from 'react';
import { getQuote, type QuoteData } from '@/lib/aggregator';
import { useWallet } from '@/lib/wallet-context';
import { RouteDisplay } from './RouteDisplay';
import { TokenSelector, type Token, TOKENS } from './TokenSelector';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export function SwapCard() {
  const [tokenIn, setTokenIn] = useState<Token>(TOKENS[0]);
  const [tokenOut, setTokenOut] = useState<Token>(TOKENS[1]);
  const [amountIn, setAmountIn] = useState('');
  const [slippage, setSlippage] = useState(0.5);
  const [quote, setQuote] = useState<QuoteData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { address: walletAddress, signTx } = useWallet();
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

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
      } catch (err: any) {
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
      // For split orders, we'd need split_swap; for now, use the best single route.
      // If the route is split across multiple paths, pick the largest one (most amount).
      const route = quote.sub_routes.reduce((best, r) =>
        parseInt(r.amount_in, 10) > parseInt(best.amount_in, 10) ? r : best,
        quote.sub_routes[0]
      );

      // Build swap steps. The api-server provides in_indices/out_indices
      // for each pool to specify input/output token positions.
      const steps = route.pool_addresses.map((pool: string, i: number) => ({
        dex_type: route.dex_types[i] ?? 'aquarius',
        pool_address: pool,
        token_in: route.path[i] ?? '',
        token_out: route.path[i + 1] ?? '',
        in_idx: route.in_indices[i] ?? 0,
        out_idx: route.out_indices[i] ?? 1,
      }));

      const buildResp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user_public_key: walletAddress,
          token_in: tokenIn.id,
          token_out: tokenOut.id,
          amount_in: Math.floor(parseFloat(amountIn) * 10 ** tokenIn.decimals).toString(),
          min_amount_out: quote.minimum_output,
          steps,
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

  return (
    <div className="w-full max-w-[480px] space-y-3">
      {/* Main Card */}
      <div className="bg-[#12131a] rounded-2xl border border-white/5 p-5 shadow-2xl shadow-black/50">
        {/* Header */}
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-base font-medium">Swap</h2>
          <div className="flex items-center gap-1.5">
            {[0.1, 0.5, 1.0].map(s => (
              <button
                key={s}
                onClick={() => setSlippage(s)}
                className={`px-2 py-0.5 rounded text-xs transition-colors ${
                  slippage === s
                    ? 'bg-blue-600/20 text-blue-400 border border-blue-500/30'
                    : 'text-gray-500 hover:text-gray-300'
                }`}
              >
                {s}%
              </button>
            ))}
          </div>
        </div>

        {/* Input Section */}
        <div className="bg-[#1a1b23] rounded-xl p-4 border border-white/5">
          <div className="flex justify-between text-xs text-gray-500 mb-2">
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
              className="flex-1 bg-transparent text-2xl font-medium outline-none placeholder-gray-600 min-w-0"
            />
            <TokenSelector
              selected={tokenIn}
              onSelect={(t) => { setTokenIn(t); setQuote(null); }}
              exclude={tokenOut.id}
            />
          </div>
        </div>

        {/* Swap Direction Button */}
        <div className="flex justify-center -my-2 relative z-10">
          <button
            onClick={swapDirection}
            className="w-9 h-9 rounded-xl bg-[#1a1b23] border border-white/10 flex items-center justify-center hover:border-blue-500/50 hover:bg-[#1e1f2a] transition-all group"
          >
            <svg className="w-4 h-4 text-gray-400 group-hover:text-blue-400 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
            </svg>
          </button>
        </div>

        {/* Output Section */}
        <div className="bg-[#1a1b23] rounded-xl p-4 border border-white/5">
          <div className="flex justify-between text-xs text-gray-500 mb-2">
            <span>You receive</span>
            {loading && (
              <span className="text-blue-400 animate-pulse">Finding best route...</span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <div className="flex-1 text-2xl font-medium min-w-0">
              {quote ? (
                <span className="text-white">{formatOutput(quote.expected_output)}</span>
              ) : (
                <span className="text-gray-600">0</span>
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
          <div className="mt-3 px-1 text-xs text-gray-500">
            1 {tokenIn.symbol} ≈{' '}
            {(parseInt(quote.expected_output, 10) / 10 ** tokenOut.decimals / parseFloat(amountIn)).toLocaleString(undefined, {
              maximumFractionDigits: 8,
            })}{' '}
            {tokenOut.symbol}
          </div>
        )}

        {/* Error */}
        {error && !loading && (
          <div className="mt-3 text-xs text-red-400/80 text-center">{error}</div>
        )}

        {/* Action Button */}
        <div className="mt-4">
          <button
            onClick={handleSwap}
            disabled={!walletAddress || !quote || loading || swapping}
            className="w-full py-3.5 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-500 hover:to-blue-400 disabled:from-gray-700 disabled:to-gray-700 disabled:text-gray-500 rounded-xl font-medium transition-all"
          >
            {swapping ? 'Swapping...' : loading ? 'Finding route...' : !amountIn ? 'Enter amount' : !walletAddress ? 'Connect wallet to swap' : !quote ? 'No route' : 'Swap'}
          </button>
        </div>

        {/* Transaction Result */}
        {txResult && (
          <div className={`mt-3 p-3 rounded-lg text-xs ${txResult.success ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400'}`}>
            {txResult.success ? (
              <div>
                ✅ Swap successful!{' '}
                <a href={`https://stellar.expert/explorer/public/tx/${txResult.hash}`} target="_blank" rel="noopener" className="underline">
                  View tx
                </a>
              </div>
            ) : (
              <div>❌ {txResult.error}</div>
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
        />
      )}
    </div>
  );
}

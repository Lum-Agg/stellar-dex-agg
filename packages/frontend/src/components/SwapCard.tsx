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
  const { address: walletAddress } = useWallet();
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

  const handleSwap = useCallback(async () => {
    if (!walletAddress || !quote) return;
    // TODO: implement swap execution
    alert('Swap execution coming soon!');
  }, [walletAddress, quote]);

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

        {/* Rate display */}
        {quote && (
          <div className="mt-3 px-1 text-xs text-gray-500">
            1 {tokenIn.symbol} ≈ {formatOutput(
              (parseInt(quote.expected_output) * 10 ** tokenIn.decimals / parseInt(amountIn || '1') / 10 ** tokenIn.decimals * 10 ** tokenOut.decimals).toFixed(0)
            )} {tokenOut.symbol}
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
            disabled={!walletAddress || !quote || loading}
            className="w-full py-3.5 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-500 hover:to-blue-400 disabled:from-gray-700 disabled:to-gray-700 disabled:text-gray-500 rounded-xl font-medium transition-all"
          >
            {loading ? 'Finding route...' : !amountIn ? 'Enter amount' : !walletAddress ? 'Connect wallet to swap' : !quote ? 'No route' : 'Swap'}
          </button>
        </div>
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

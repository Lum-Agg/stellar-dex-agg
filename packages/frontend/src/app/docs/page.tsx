'use client';

import { useState } from 'react';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

const TOKENS: Record<string, string> = {
  XLM: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
  USDC: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
  EURC: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
};

export default function DocsPage() {
  return (
    <div className="max-w-4xl mx-auto">
      <h1 className="text-3xl font-bold mb-2">API Documentation</h1>
      <p className="text-gray-400 mb-8">
        LumAgg aggregates liquidity across Soroswap, Aquarius, Phoenix, Sushi V3, Comet and Stellar Classic DEX.
      </p>

      <div className="mb-6 p-4 rounded-lg bg-white/5 border border-white/10">
        <p className="text-sm text-gray-300">
          <span className="font-mono text-blue-400">Base URL:</span>{' '}
          <code className="bg-black/30 px-2 py-0.5 rounded">{API_URL}</code>
        </p>
        <p className="text-sm text-gray-300 mt-1">
          <span className="font-mono text-blue-400">Contract:</span>{' '}
          <code className="bg-black/30 px-2 py-0.5 rounded text-xs">CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K</code>
        </p>
      </div>

      {/* Endpoints */}
      <div className="space-y-8">
        <EndpointSection
          method="GET"
          path="/api/v1/quote"
          description="Get the best swap route and expected output amount."
          params={[
            { name: 'token_in', type: 'string', required: true, desc: 'Input token contract address' },
            { name: 'token_out', type: 'string', required: true, desc: 'Output token contract address' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Input amount in stroops (7 decimals)' },
            { name: 'slippage', type: 'number', required: false, desc: 'Slippage tolerance (e.g. 0.5 = 0.5%)' },
          ]}
          tryIt={<QuoteTryIt />}
        />

        <EndpointSection
          method="POST"
          path="/api/v1/swap"
          description="Build an unsigned swap transaction for the user to sign."
          params={[
            { name: 'token_in', type: 'string', required: true, desc: 'Input token contract address' },
            { name: 'token_out', type: 'string', required: true, desc: 'Output token contract address' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Input amount in stroops' },
            { name: 'slippage', type: 'number', required: true, desc: 'Slippage tolerance (e.g. 0.5)' },
            { name: 'user_public_key', type: 'string', required: true, desc: 'User Stellar public key (G...)' },
          ]}
        />

        <EndpointSection
          method="GET"
          path="/api/v1/tokens"
          description="List supported tokens with their contract addresses."
          params={[]}
          tryIt={<TokensTryIt />}
        />

        <EndpointSection
          method="GET"
          path="/api/v1/health"
          description="Check API health and adapter status."
          params={[]}
          tryIt={<HealthTryIt />}
        />

        <EndpointSection
          method="POST"
          path="/api/v1/build_tx"
          description="Build an unsigned transaction from one or more quote results. Use for arbitrage: call quote() twice (A→B, B→A), then send both legs here to get a single atomic tx."
          params={[
            { name: 'user_public_key', type: 'string', required: true, desc: 'User Stellar public key (G...)' },
            { name: 'legs', type: 'array', required: true, desc: 'Array of swap legs (each with token_in, token_out, amount_in, min_amount_out)' },
          ]}
          tryIt={<BuildTxTryIt />}
        />
      </div>

      {/* Supported DEXes */}
      <div className="mt-12 p-6 rounded-lg bg-white/5 border border-white/10">
        <h2 className="text-xl font-bold mb-4">Supported DEXes</h2>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-4 text-sm">
          {[
            { name: 'Soroswap', pools: '192', type: 'AMM (xy=k)' },
            { name: 'Aquarius', pools: '282', type: 'AMM + Stable + CLMM' },
            { name: 'Phoenix', pools: '11', type: 'AMM' },
            { name: 'Sushi V3', pools: '20', type: 'CLMM (tick-based)' },
            { name: 'Comet', pools: '1', type: 'Weighted (Balancer)' },
            { name: 'Classic DEX', pools: '∞', type: 'Orderbook + LP' },
          ].map((dex) => (
            <div key={dex.name} className="p-3 rounded bg-white/5">
              <div className="font-medium text-white">{dex.name}</div>
              <div className="text-gray-500 text-xs">{dex.type}</div>
              <div className="text-gray-400 text-xs mt-1">{dex.pools} pools</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function EndpointSection({
  method,
  path,
  description,
  params,
  tryIt,
}: {
  method: string;
  path: string;
  description: string;
  params: { name: string; type: string; required: boolean; desc: string }[];
  tryIt?: React.ReactNode;
}) {
  return (
    <div className="p-6 rounded-lg bg-white/5 border border-white/10">
      <div className="flex items-center gap-3 mb-2">
        <span className={`px-2 py-0.5 rounded text-xs font-bold ${method === 'GET' ? 'bg-green-500/20 text-green-400' : 'bg-blue-500/20 text-blue-400'}`}>
          {method}
        </span>
        <code className="text-white font-mono">{path}</code>
      </div>
      <p className="text-gray-400 text-sm mb-4">{description}</p>

      {params.length > 0 && (
        <div className="mb-4">
          <h4 className="text-xs font-bold text-gray-500 uppercase mb-2">Parameters</h4>
          <div className="space-y-1">
            {params.map((p) => (
              <div key={p.name} className="flex items-start gap-2 text-sm">
                <code className="text-blue-300 font-mono text-xs bg-black/30 px-1.5 py-0.5 rounded">{p.name}</code>
                <span className="text-gray-500 text-xs">{p.type}{p.required ? '' : '?'}</span>
                <span className="text-gray-400 text-xs">— {p.desc}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {tryIt}
    </div>
  );
}

function QuoteTryIt() {
  const [tokenIn, setTokenIn] = useState('XLM');
  const [tokenOut, setTokenOut] = useState('USDC');
  const [amount, setAmount] = useState('100');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const amountStroops = (parseFloat(amount) * 10_000_000).toFixed(0);
    const url = `${API_URL}/api/v1/quote?token_in=${TOKENS[tokenIn]}&token_out=${TOKENS[tokenOut]}&amount_in=${amountStroops}`;
    try {
      const resp = await fetch(url);
      const data = await resp.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (e: any) {
      setResult(`Error: ${e.message}`);
    }
    setLoading(false);
  };

  return (
    <div className="mt-4 pt-4 border-t border-white/10">
      <h4 className="text-xs font-bold text-gray-500 uppercase mb-3">Try it</h4>
      <div className="flex flex-wrap gap-2 items-end mb-3">
        <div>
          <label className="text-xs text-gray-500 block mb-1">From</label>
          <select value={tokenIn} onChange={(e) => setTokenIn(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm">
            {Object.keys(TOKENS).map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div>
          <label className="text-xs text-gray-500 block mb-1">To</label>
          <select value={tokenOut} onChange={(e) => setTokenOut(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm">
            {Object.keys(TOKENS).map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div>
          <label className="text-xs text-gray-500 block mb-1">Amount</label>
          <input value={amount} onChange={(e) => setAmount(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm w-24" />
        </div>
        <button onClick={run} disabled={loading} className="px-3 py-1 bg-blue-600 hover:bg-blue-500 rounded text-sm font-medium disabled:opacity-50">
          {loading ? '...' : 'Send'}
        </button>
      </div>
      {result && (
        <pre className="bg-black/60 rounded p-3 text-xs text-green-300 overflow-x-auto max-h-64 overflow-y-auto">{result}</pre>
      )}
    </div>
  );
}

function TokensTryIt() {
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    try {
      const resp = await fetch(`${API_URL}/api/v1/tokens`);
      const data = await resp.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (e: any) {
      setResult(`Error: ${e.message}`);
    }
    setLoading(false);
  };

  return (
    <div className="mt-4 pt-4 border-t border-white/10">
      <button onClick={run} disabled={loading} className="px-3 py-1 bg-blue-600 hover:bg-blue-500 rounded text-sm font-medium disabled:opacity-50">
        {loading ? '...' : 'Try it'}
      </button>
      {result && (
        <pre className="mt-3 bg-black/60 rounded p-3 text-xs text-green-300 overflow-x-auto max-h-48 overflow-y-auto">{result}</pre>
      )}
    </div>
  );
}

function HealthTryIt() {
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    try {
      const resp = await fetch(`${API_URL}/api/v1/health`);
      const data = await resp.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (e: any) {
      setResult(`Error: ${e.message}`);
    }
    setLoading(false);
  };

  return (
    <div className="mt-4 pt-4 border-t border-white/10">
      <button onClick={run} disabled={loading} className="px-3 py-1 bg-blue-600 hover:bg-blue-500 rounded text-sm font-medium disabled:opacity-50">
        {loading ? '...' : 'Try it'}
      </button>
      {result && (
        <pre className="mt-3 bg-black/60 rounded p-3 text-xs text-green-300 overflow-x-auto">{result}</pre>
      )}
    </div>
  );
}

function BuildTxTryIt() {
  const [tokenIn, setTokenIn] = useState('XLM');
  const [tokenOut, setTokenOut] = useState('USDC');
  const [amount, setAmount] = useState('100');
  const [pubKey, setPubKey] = useState('GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const amountStroops = (parseFloat(amount) * 10_000_000).toFixed(0);

    try {
      // Step 1: Quote leg1 (token → intermediate)
      const q1Url = `${API_URL}/api/v1/quote?token_in=${TOKENS[tokenIn]}&token_out=${TOKENS[tokenOut]}&amount_in=${amountStroops}`;
      const q1Resp = await fetch(q1Url);
      const q1 = await q1Resp.json();

      if (!q1.success || !q1.data) {
        setResult(JSON.stringify({ error: 'Leg 1 quote failed', details: q1 }, null, 2));
        setLoading(false);
        return;
      }

      // Step 2: Quote leg2 (intermediate → token)
      const q2Url = `${API_URL}/api/v1/quote?token_in=${TOKENS[tokenOut]}&token_out=${TOKENS[tokenIn]}&amount_in=${q1.data.expected_output}`;
      const q2Resp = await fetch(q2Url);
      const q2 = await q2Resp.json();

      if (!q2.success || !q2.data) {
        setResult(JSON.stringify({ error: 'Leg 2 quote failed', details: q2 }, null, 2));
        setLoading(false);
        return;
      }

      // Calculate profit
      const finalAmount = parseInt(q2.data.expected_output);
      const startAmount = parseInt(amountStroops);
      const profit = finalAmount - startAmount;
      const profitPct = ((profit / startAmount) * 100).toFixed(4);

      // Step 3: Build tx with both legs
      const buildResp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user_public_key: pubKey,
          legs: [
            { token_in: TOKENS[tokenIn], token_out: TOKENS[tokenOut], amount_in: amountStroops, min_amount_out: q1.data.minimum_output },
            { token_in: TOKENS[tokenOut], token_out: TOKENS[tokenIn], amount_in: q1.data.expected_output, min_amount_out: String(startAmount) },
          ],
        }),
      });
      const buildResult = await buildResp.json();

      setResult(JSON.stringify({
        leg1: { route: q1.data.sub_routes, output: q1.data.expected_output },
        leg2: { route: q2.data.sub_routes, output: q2.data.expected_output },
        profit: { amount: profit, pct: `${profitPct}%`, profitable: profit > 0 },
        tx: buildResult.data,
      }, null, 2));
    } catch (e: any) {
      setResult(`Error: ${e.message}`);
    }
    setLoading(false);
  };

  return (
    <div className="mt-4 pt-4 border-t border-white/10">
      <h4 className="text-xs font-bold text-gray-500 uppercase mb-2">Try it — Arb Example (quote + quote + build_tx)</h4>
      <p className="text-xs text-gray-500 mb-3">Calls quote twice (A→B, B→A) then builds atomic tx</p>
      <div className="flex flex-wrap gap-2 items-end mb-3">
        <div>
          <label className="text-xs text-gray-500 block mb-1">Token A</label>
          <select value={tokenIn} onChange={(e) => setTokenIn(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm">
            {Object.keys(TOKENS).map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div>
          <label className="text-xs text-gray-500 block mb-1">Token B</label>
          <select value={tokenOut} onChange={(e) => setTokenOut(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm">
            {Object.keys(TOKENS).map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div>
          <label className="text-xs text-gray-500 block mb-1">Amount</label>
          <input value={amount} onChange={(e) => setAmount(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm w-24" />
        </div>
        <button onClick={run} disabled={loading} className="px-3 py-1 bg-purple-600 hover:bg-purple-500 rounded text-sm font-medium disabled:opacity-50">
          {loading ? '...' : 'Find Arb'}
        </button>
      </div>
      {result && (
        <pre className="bg-black/60 rounded p-3 text-xs text-green-300 overflow-x-auto max-h-80 overflow-y-auto">{result}</pre>
      )}
    </div>
  );
}

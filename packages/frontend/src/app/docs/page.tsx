'use client';

import { useState, type ReactNode } from 'react';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';
import { BuildTxCodeSample } from '@/components/BuildTxCodeSample';
import { GITHUB_REPO_URL } from '@/lib/site';

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
      <p className="text-gray-400 mb-4">
        LumAgg aggregates liquidity across Soroswap, Aquarius, Phoenix, Sushi V3, Comet and Stellar Classic DEX.
      </p>
      <p className="text-sm text-gray-500 mb-8">
        Source code and architecture docs:{' '}
        <a
          href={GITHUB_REPO_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="text-blue-400 hover:text-blue-300 underline underline-offset-2"
        >
          github.com/ligulfzhou/stellar-dex-agg
        </a>
      </p>

      <DisclaimerBanner className="mb-6" />

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
          path="/api/v1/health"
          description="Check API health and adapter status."
          params={[]}
          tryIt={<HealthTryIt />}
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
          path="/api/v1/build_tx"
          description="Optional helper: LumAgg can simulate and return unsigned XDR for you. If you assemble txs yourself, only GET /quote is required — see the sample below (uses @stellar/stellar-sdk + Soroban RPC simulate, not this endpoint)."
          params={[
            { name: 'user_public_key', type: 'string', required: true, desc: 'User Stellar public key (G...)' },
            { name: 'token_in', type: 'string', required: true, desc: 'Input token contract address' },
            { name: 'token_out', type: 'string', required: true, desc: 'Final output token contract address' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Total input in stroops (sum of sub-route amounts)' },
            { name: 'min_amount_out', type: 'string', required: true, desc: 'Minimum acceptable output' },
            { name: 'sub_routes', type: 'array', required: true, desc: 'Legs: [{amount_in, steps: [{dex_type, pool_address, token_in, token_out, in_idx, out_idx}]}]' },
          ]}
          beforeTryIt={
            <BuildTxCodeSample
              apiUrl={API_URL}
              rpcUrl={process.env.NEXT_PUBLIC_SOROBAN_RPC_URL || 'https://soroban-rpc.mainnet.stellar.gateway.fm'}
            />
          }
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
  beforeTryIt,
  tryIt,
}: {
  method: string;
  path: string;
  description: string;
  params: { name: string; type: string; required: boolean; desc: string }[];
  beforeTryIt?: ReactNode;
  tryIt?: ReactNode;
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

      {beforeTryIt}

      {tryIt}
    </div>
  );
}

function QuoteTryIt() {
  const [tokenIn, setTokenIn] = useState('XLM');
  const [tokenOut, setTokenOut] = useState('USDC');
  const [amount, setAmount] = useState('100');
  const [slippage, setSlippage] = useState('0.5');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const amountStroops = (parseFloat(amount) * 10_000_000).toFixed(0);
    const url = `${API_URL}/api/v1/quote?token_in=${TOKENS[tokenIn]}&token_out=${TOKENS[tokenOut]}&amount_in=${amountStroops}&slippage=${slippage}`;
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
        <div>
          <label className="text-xs text-gray-500 block mb-1">Slippage %</label>
          <input value={slippage} onChange={(e) => setSlippage(e.target.value)} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-sm w-16" />
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
  const [reqJson, setReqJson] = useState(JSON.stringify({
    user_public_key: "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY",
    token_in: "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
    token_out: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
    amount_in: "1000000000",
    min_amount_out: "140000000",
    sub_routes: [{
      amount_in: "1000000000",
      steps: [{
        dex_type: "aquarius",
        pool_address: "CDKVJYMN34ZIEXSLNFYHVAFF6M6FM5E2U6OHXOTBKH2WLBULXOE53YDP",
        token_in: "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        token_out: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        in_idx: 0, out_idx: 1
      }]
    }]
  }, null, 2));
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    try {
      const body = JSON.parse(reqJson);
      const resp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
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
      <div className="space-y-2 mb-3">
        <div>
          <label className="text-xs text-gray-500 block mb-1">Request Body (JSON)</label>
          <textarea value={reqJson} onChange={(e) => setReqJson(e.target.value)} rows={12} className="bg-black/40 border border-white/10 rounded px-2 py-1 text-xs w-full font-mono" />
        </div>
        <button onClick={run} disabled={loading} className="px-3 py-1 bg-blue-600 hover:bg-blue-500 rounded text-sm font-medium disabled:opacity-50">
          {loading ? '...' : 'Build TX'}
        </button>
      </div>
      {result && (
        <pre className="bg-black/60 rounded p-3 text-xs text-green-300 overflow-x-auto max-h-48 overflow-y-auto">{result}</pre>
      )}
    </div>
  );
}

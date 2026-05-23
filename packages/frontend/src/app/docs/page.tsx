'use client';

import { useState, type ReactNode } from 'react';
import { BuildTxCodeSample } from '@/components/BuildTxCodeSample';
import { GITHUB_REPO_URL } from '@/lib/site';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';
const AGGREGATOR =
  'CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K';

const TOKENS: Record<string, string> = {
  XLM: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
  USDC: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
  EURC: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
};

type Param = { name: string; type: string; required: boolean; desc: string };

export default function DocsPage() {
  return (
    <article className="docs-page">
      <header className="docs-intro">
        <h1 className="docs-title">API Documentation</h1>
        <p className="docs-lead">
          Liquidity aggregator across Soroswap, Aquarius, Phoenix, Sushi V3, Comet and Classic
          DEX.
        </p>
        <p className="docs-meta">
          <a href={GITHUB_REPO_URL} target="_blank" rel="noopener noreferrer">
            github.com/ligulfzhou/stellar-dex-agg
          </a>
        </p>
        <dl className="docs-ref">
          <div className="docs-ref-row">
            <dt>Base URL</dt>
            <dd>
              <code>{API_URL}</code>
            </dd>
          </div>
          <div className="docs-ref-row">
            <dt>Contract</dt>
            <dd>
              <code>{AGGREGATOR}</code>
            </dd>
          </div>
        </dl>
      </header>

      <div className="docs-list">
        <Endpoint
          method="GET"
          path="/api/v1/health"
          description="Health check."
          tryIt={<PingTryIt path="/api/v1/health" />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/tokens"
          description="Supported tokens."
          tryIt={<PingTryIt path="/api/v1/tokens" />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/quote"
          description="Best route and expected output."
          params={[
            { name: 'token_in', type: 'string', required: true, desc: 'Input token (contract id)' },
            { name: 'token_out', type: 'string', required: true, desc: 'Output token (contract id)' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Stroops, 7 decimals' },
            { name: 'slippage', type: 'number', required: false, desc: 'Percent, e.g. 0.5' },
          ]}
          tryIt={<QuoteTryIt />}
        />

        <Endpoint
          method="POST"
          path="/api/v1/build_tx"
          description="Optional: unsigned XDR from a quote. You can also assemble txs locally (see sample)."
          params={[
            { name: 'user_public_key', type: 'string', required: true, desc: 'G... address' },
            { name: 'token_in', type: 'string', required: true, desc: 'Input token' },
            { name: 'token_out', type: 'string', required: true, desc: 'Output token' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Total stroops in' },
            { name: 'min_amount_out', type: 'string', required: true, desc: 'Min stroops out' },
            { name: 'sub_routes', type: 'array', required: true, desc: 'From GET /quote response' },
          ]}
          extra={
            <BuildTxCodeSample
              apiUrl={API_URL}
              rpcUrl={
                process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ||
                'https://soroban-rpc.mainnet.stellar.gateway.fm'
              }
            />
          }
          tryIt={<BuildTxTryIt />}
        />
      </div>

      <footer className="docs-card docs-dexes">
        <h2 className="docs-section-label">Supported DEXes</h2>
        <ul className="docs-dex-list">
          {[
            ['Soroswap', 'AMM'],
            ['Aquarius', 'AMM · Stable · CLMM'],
            ['Phoenix', 'AMM'],
            ['Sushi V3', 'CLMM'],
            ['Comet', 'Weighted'],
            ['Classic', 'SDEX'],
          ].map(([name, type]) => (
            <li key={name}>
              <strong>{name}</strong>
              <span>{type}</span>
            </li>
          ))}
        </ul>
      </footer>
    </article>
  );
}

function Endpoint({
  method,
  path,
  description,
  params = [],
  extra,
  tryIt,
}: {
  method: string;
  path: string;
  description: string;
  params?: Param[];
  extra?: ReactNode;
  tryIt?: ReactNode;
}) {
  const isGet = method === 'GET';

  return (
    <section className="docs-card">
      <div className="docs-endpoint-head">
        <span className={isGet ? 'docs-method docs-method--get' : 'docs-method docs-method--post'}>
          {method}
        </span>
        <code className="docs-path">{path}</code>
      </div>
      <p className="docs-desc">{description}</p>

      {params.length > 0 && (
        <div className="docs-params">
          <p className="docs-section-label">Query / body</p>
          <ul>
            {params.map((p) => (
              <li key={p.name} className="docs-param">
                <code>{p.name}</code>
                <span className="docs-param-type">
                  {p.type}
                  {p.required ? '' : '?'}
                </span>
                <span className="docs-param-desc">{p.desc}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {extra && <div className="docs-extra">{extra}</div>}

      {tryIt && (
        <div className="docs-try">
          <p className="docs-section-label">Try it</p>
          {tryIt}
        </div>
      )}
    </section>
  );
}

function PingTryIt({ path }: { path: string }) {
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    try {
      const resp = await fetch(`${API_URL}${path}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <button type="button" className="docs-btn" onClick={run} disabled={loading}>
        {loading ? '…' : 'Send'}
      </button>
      {result && <pre className="docs-out">{result}</pre>}
    </>
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
    const stroops = (parseFloat(amount) * 10_000_000).toFixed(0);
    const q = new URLSearchParams({
      token_in: TOKENS[tokenIn],
      token_out: TOKENS[tokenOut],
      amount_in: stroops,
      slippage,
    });
    try {
      const resp = await fetch(`${API_URL}/api/v1/quote?${q}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="From">
          <select className="docs-input" value={tokenIn} onChange={(e) => setTokenIn(e.target.value)}>
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="To">
          <select className="docs-input" value={tokenOut} onChange={(e) => setTokenOut(e.target.value)}>
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Amount">
          <input className="docs-input docs-input--narrow" value={amount} onChange={(e) => setAmount(e.target.value)} />
        </Field>
        <Field label="Slippage %">
          <input className="docs-input docs-input--narrow" value={slippage} onChange={(e) => setSlippage(e.target.value)} />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function BuildTxTryIt() {
  const [body, setBody] = useState(
    JSON.stringify(
      {
        user_public_key: 'GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY',
        token_in: TOKENS.XLM,
        token_out: TOKENS.USDC,
        amount_in: '1000000000',
        min_amount_out: '140000000',
        sub_routes: [
          {
            amount_in: '1000000000',
            steps: [
              {
                dex_type: 'aquarius',
                pool_address: 'CDKVJYMN34ZIEXSLNFYHVAFF6M6FM5E2U6OHXOTBKH2WLBULXOE53YDP',
                token_in: TOKENS.XLM,
                token_out: TOKENS.USDC,
                in_idx: 0,
                out_idx: 1,
              },
            ],
          },
        ],
      },
      null,
      2,
    ),
  );
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    try {
      const resp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      });
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <textarea className="docs-textarea" value={body} onChange={(e) => setBody(e.target.value)} rows={8} />
      <button type="button" className="docs-btn docs-btn--spaced" onClick={run} disabled={loading}>
        {loading ? '…' : 'Send'}
      </button>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="docs-field">
      <span>{label}</span>
      {children}
    </label>
  );
}

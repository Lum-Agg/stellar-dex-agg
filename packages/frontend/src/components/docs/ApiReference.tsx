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

const DEMO_USER = 'GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY';

type QuoteSubRoute = {
  path: string[];
  pool_addresses: string[];
  dex_types: string[];
  in_indices: number[];
  out_indices: number[];
  amount_in: string;
};

type QuotePayload = {
  amount_in: string;
  minimum_output: string;
  sub_routes: QuoteSubRoute[];
};

function quoteToBuildTxBody(
  userPublicKey: string,
  tokenIn: string,
  tokenOut: string,
  quote: QuotePayload,
) {
  return {
    user_public_key: userPublicKey,
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: quote.amount_in,
    min_amount_out: quote.minimum_output,
    sub_routes: quote.sub_routes.map((sr) => ({
      amount_in: sr.amount_in,
      steps: sr.dex_types.map((dexType, i) => ({
        dex_type: dexType,
        pool_address: sr.pool_addresses[i],
        token_in: sr.path[i],
        token_out: sr.path[i + 1],
        in_idx: sr.in_indices[i],
        out_idx: sr.out_indices[i],
      })),
    })),
  };
}

type Param = { name: string; type: string; required: boolean; desc: string };

export function ApiReference() {
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
            github.com/Lum-Agg/stellar-dex-agg
          </a>
          {' · '}
          <a
            href={`${GITHUB_REPO_URL}/blob/main/docs/openapi.yaml`}
            target="_blank"
            rel="noopener noreferrer"
          >
            OpenAPI
          </a>
          {' · '}
          <a
            href={`${GITHUB_REPO_URL}/blob/main/docs/integrator-guide.md`}
            target="_blank"
            rel="noopener noreferrer"
          >
            Integrator guide
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
          path="/api/v1/swaps"
          description="Recent aggregator swaps for a wallet (indexer DB)."
          params={[
            { name: 'user', type: 'string', required: true, desc: 'G... address' },
            { name: 'limit', type: 'number', required: false, desc: '1–50, default 20' },
          ]}
          tryIt={<SwapsTryIt />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/prices"
          description="Latest USDC mark per token (sampled ticks or on-demand quote)."
          params={[
            { name: 'ids', type: 'string', required: true, desc: 'Comma-separated contract ids (max 50)' },
          ]}
          tryIt={<PricesTryIt />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/prices/history"
          description="Sampled USDC price ticks for sparklines."
          params={[
            { name: 'id', type: 'string', required: true, desc: 'Token contract id' },
            { name: 'range', type: '24h | 7d', required: true, desc: 'History window' },
          ]}
          tryIt={<PriceHistoryTryIt />}
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
            { name: 'prefer_soroban', type: '0 | 1', required: false, desc: '1 = Soroban AMMs only (exclude Classic SDEX)' },
          ]}
          tryIt={<QuoteTryIt />}
        />

        <Endpoint
          method="POST"
          path="/api/v1/build_tx"
          description="Optional: unsigned XDR from a quote. Try-it fetches GET /quote first, then posts the live sub_routes here."
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

function SwapsTryIt() {
  const [user, setUser] = useState(DEMO_USER);
  const [limit, setLimit] = useState('');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const q = new URLSearchParams({ user: user.trim() });
    if (limit.trim()) q.set('limit', limit.trim());
    try {
      const resp = await fetch(`${API_URL}/api/v1/swaps?${q}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="User (G…)">
          <input
            className="docs-input"
            value={user}
            onChange={(e) => setUser(e.target.value)}
            spellCheck={false}
          />
        </Field>
        <Field label="Limit">
          <input
            className="docs-input docs-input--narrow"
            value={limit}
            onChange={(e) => setLimit(e.target.value)}
            placeholder="20"
          />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function PricesTryIt() {
  const [ids, setIds] = useState(`${TOKENS.XLM},${TOKENS.USDC}`);
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const q = new URLSearchParams({ ids: ids.trim() });
    try {
      const resp = await fetch(`${API_URL}/api/v1/prices?${q}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="Ids (comma-separated)">
          <input
            className="docs-input"
            value={ids}
            onChange={(e) => setIds(e.target.value)}
            spellCheck={false}
          />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function PriceHistoryTryIt() {
  const [id, setId] = useState(TOKENS.XLM);
  const [range, setRange] = useState<'24h' | '7d'>('24h');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const q = new URLSearchParams({ id: id.trim(), range });
    try {
      const resp = await fetch(`${API_URL}/api/v1/prices/history?${q}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="Token id">
          <input
            className="docs-input"
            value={id}
            onChange={(e) => setId(e.target.value)}
            spellCheck={false}
          />
        </Field>
        <Field label="Range">
          <select className="docs-input docs-input--narrow" value={range} onChange={(e) => setRange(e.target.value as '24h' | '7d')}>
            <option value="24h">24h</option>
            <option value="7d">7d</option>
          </select>
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
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
  const [userKey, setUserKey] = useState(DEMO_USER);
  const [tokenIn, setTokenIn] = useState('XLM');
  const [tokenOut, setTokenOut] = useState('USDC');
  const [amount, setAmount] = useState('1');
  const [slippage, setSlippage] = useState('1');
  const [requestBody, setRequestBody] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    setRequestBody(null);
    const tokenInId = TOKENS[tokenIn];
    const tokenOutId = TOKENS[tokenOut];
    const stroops = (parseFloat(amount) * 10_000_000).toFixed(0);
    try {
      const quoteResp = await fetch(
        `${API_URL}/api/v1/quote?${new URLSearchParams({
          token_in: tokenInId,
          token_out: tokenOutId,
          amount_in: stroops,
          slippage,
        })}`,
      );
      const quoteJson = await quoteResp.json();
      if (!quoteJson.success || !quoteJson.data?.sub_routes?.length) {
        setResult(JSON.stringify(quoteJson, null, 2));
        setLoading(false);
        return;
      }

      const buildBody = quoteToBuildTxBody(userKey.trim(), tokenInId, tokenOutId, quoteJson.data);
      setRequestBody(JSON.stringify(buildBody, null, 2));

      const buildResp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(buildBody),
      });
      setResult(JSON.stringify(await buildResp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <p className="docs-hint">
        Calls <code>GET /quote</code> first so pool addresses and token indices match live routing.
      </p>
      <div className="docs-form-row">
        <Field label="User (G…)">
          <input
            className="docs-input"
            value={userKey}
            onChange={(e) => setUserKey(e.target.value)}
            spellCheck={false}
          />
        </Field>
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
          {loading ? '…' : 'Quote → build_tx'}
        </button>
      </div>
      {requestBody && (
        <>
          <p className="docs-hint">POST /api/v1/build_tx body (from quote):</p>
          <pre className="docs-out">{requestBody}</pre>
        </>
      )}
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

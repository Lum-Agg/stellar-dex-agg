import Link from 'next/link';
import { DOCUMENTATION_URL, GITHUB_REPO_URL } from '@/lib/site';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

const WHY_CARDS = [
  {
    title: 'Best route',
    body: 'Quotes across Soroban AMMs and Classic SDEX with split routing when it helps execution.',
  },
  {
    title: 'Integrator-ready',
    body: 'REST quote → build_tx flow, OpenAPI spec, npm SDK, and partner API keys.',
  },
  {
    title: 'Self-hostable',
    body: 'Open-source aggregator contract and api-server — run your own stack if you need to.',
  },
] as const;

const QUICKSTART = `API=${API_URL}
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

# 1) Quote 1 XLM → USDC
curl -sG "$API/api/v1/quote" \\
  --data-urlencode "token_in=$XLM" \\
  --data-urlencode "token_out=$USDC" \\
  --data-urlencode "amount_in=10000000" \\
  --data-urlencode "slippage=0.5"

# 2) Map sub_routes → POST /api/v1/build_tx (see API reference)`;

export default function DocsOverviewPage() {
  return (
    <article className="docs-page">
      <header className="docs-intro">
        <h1 className="docs-title">Developer documentation</h1>
        <p className="docs-lead">
          Integrate LumAgg swap routing on Stellar — quote, build unsigned XDR, and sign in your
          wallet or bot.
        </p>
        <aside className="docs-guide-callout">
          <div>
            <strong>Looking for the complete LumAgg documentation?</strong>
            <span>
              Visit the documentation site for integration, deployment, arbitrage, and contract
              guides.
            </span>
          </div>
          <a
            href={DOCUMENTATION_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="docs-btn docs-btn--inline"
          >
            Visit documentation
          </a>
        </aside>
        <dl className="docs-ref">
          <div className="docs-ref-row">
            <dt>Base URL</dt>
            <dd>
              <code>{API_URL}</code>
            </dd>
          </div>
          <div className="docs-ref-row">
            <dt>Flow</dt>
            <dd>
              <code>GET /quote</code> → <code>POST /build_tx</code> → sign → submit
            </dd>
          </div>
        </dl>
      </header>

      <section className="docs-card-grid">
        {WHY_CARDS.map((card) => (
          <div key={card.title} className="docs-card docs-card--flat">
            <h2 className="docs-card-title">{card.title}</h2>
            <p className="docs-desc">{card.body}</p>
          </div>
        ))}
      </section>

      <section className="docs-card">
        <h2 className="docs-section-label">Quickstart</h2>
        <p className="docs-desc">
          Fetch a live quote, then use the interactive{' '}
          <Link href="/docs/api" className="docs-inline-link">
            API reference
          </Link>{' '}
          to try <code>build_tx</code> with the returned <code>sub_routes</code>.
        </p>
        <pre className="docs-code-block">{QUICKSTART}</pre>
        <div className="docs-actions">
          <Link href="/docs/api" className="docs-btn docs-btn--inline">
            Open API reference
          </Link>
          <a
            href={`${GITHUB_REPO_URL}/blob/main/docs/integrator-guide.md`}
            target="_blank"
            rel="noopener noreferrer"
            className="docs-btn docs-btn--ghost"
          >
            Full integrator guide
          </a>
        </div>
      </section>

      <section className="docs-card">
        <h2 className="docs-section-label">Rate limits</h2>
        <div className="docs-table-wrap">
          <table className="docs-table">
            <thead>
              <tr>
                <th>Tier</th>
                <th>Limit</th>
                <th>Auth</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Anonymous</td>
                <td>10 req/s per IP</td>
                <td>none</td>
              </tr>
              <tr>
                <td>Partner</td>
                <td>60 req/s per key</td>
                <td>
                  <code>X-API-Key</code> header
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p className="docs-hint">
          Use <code>prefer_soroban=1</code> on <code>/quote</code> for Soroban AMMs only (no
          Classic SDEX). Default may return pure classic or pure Soroban; never mixed hops.
        </p>
      </section>

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

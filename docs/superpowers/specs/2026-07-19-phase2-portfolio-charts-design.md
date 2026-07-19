# LumAgg Phase 2: Portfolio + Quote-Sampled Charts

**Date:** 2026-07-19  
**Status:** Approved for planning  
**Depends on:** Phase 1 (`GET /api/v1/swaps`, homepage History)

## Goal

Ship a lean Jupiter/Titan-style **Portfolio** surface: wallet holdings valued via the existing quote engine, plus **self-sampled** USDC price ticks for simple 24h/7d sparklines — without Limit/DCA or external market-data APIs for marks.

## Non-goals

- Limit orders, DCA, TWAP
- Exact-out / venue filters (Phase 1.1)
- Cost basis / PnL / tax lots
- Full token detail pages, TradingView-style candles, volume charts
- CoinGecko (or other) as the primary mark for holdings (stats enrichment may keep separate historical XLM/USD as today)
- Permanent pruning of price ticks by default

## Product decisions (locked)

| Decision | Choice |
|----------|--------|
| Scope depth | Lean MVP |
| Placement | Homepage holdings summary + full `/portfolio` page |
| Mark / valuation | Quote engine (`token → USDC`, fallback `→ XLM` then XLM→USDC) |
| History for charts | Self-sampled ticks in SQLite inside api-server |
| Sampler architecture | Approach A: in-process sampler + SQLite |
| Tick retention | **Permanent by default**; optional `PRICE_RETENTION_DAYS` if set |

## Architecture

```
api-server (background task)
  every PRICE_SAMPLE_SECS (default 600)
    quote whitelist tokens → USDC (or via XLM)
    INSERT into price_ticks SQLite (PRICE_DB_PATH)

Frontend HoldingsSummary / Portfolio
  balances ← GET /api/v1/balances (existing)
  marks    ← GET /api/v1/prices
  sparklines ← GET /api/v1/prices/history?id=&range=24h|7d
```

## Storage

SQLite at `PRICE_DB_PATH` (required for sampler + history; if unset, prices endpoints may on-demand quote only / return empty history).

```sql
CREATE TABLE IF NOT EXISTS price_ticks (
  token TEXT NOT NULL,
  ts INTEGER NOT NULL,
  price_usdc REAL NOT NULL,
  via TEXT NOT NULL,
  PRIMARY KEY (token, ts)
);
CREATE INDEX IF NOT EXISTS idx_price_ticks_token_ts ON price_ticks(token, ts DESC);
```

- **Retention:** no automatic delete unless `PRICE_RETENTION_DAYS` is set to a positive integer.
- **Whitelist:** priority tokens (XLM, USDC, EURC, AQUA, …) plus top N from token registry (default N=30). Env: `PRICE_SAMPLE_TOKEN_LIMIT`.

## Sampler

- Spawned from api-server when `PRICE_SAMPLER` is not `0` and quote engine is available.
- Interval: `PRICE_SAMPLE_SECS` default **600**.
- Per token:
  1. If token is USDC SAC → `price_usdc = 1.0`, `via = "usdc"`.
  2. Else try exact-in quote of `10^decimals` units into USDC → `price_usdc`.
  3. Else quote into XLM, then multiply by current XLM→USDC mark → `via = "xlm"`.
  4. On failure: log + skip.
- Must not block or crash the HTTP server on quote failures.

## API

### `GET /api/v1/prices`

| Param | Notes |
|-------|--------|
| `ids` | Comma-separated contract ids (required for batch). Cap e.g. 50. |

Returns latest tick per id. If no tick exists, **on-demand quote once**, optionally persist a tick, then return.

```json
{
  "success": true,
  "data": {
    "prices": [
      { "id": "CAS3…", "price_usdc": 0.42, "ts": 1710000000, "via": "usdc" }
    ]
  }
}
```

Missing/unpriceable ids omitted or returned with `null` price + error field — prefer omit + client shows `—`.

### `GET /api/v1/prices/history`

| Param | Notes |
|-------|--------|
| `id` | Required token id |
| `range` | `24h` \| `7d` (MVP); more ranges later |

```json
{
  "success": true,
  "data": {
    "id": "CAS3…",
    "range": "24h",
    "points": [{ "ts": 1710000000, "price_usdc": 0.42 }]
  }
}
```

Empty points → 200 with `points: []` (UI shows `—`).

### Existing

- `GET /api/v1/balances` — unchanged; Portfolio composes client-side.

### SDK / docs

- `@lumagg/sdk`: `getPrices`, `getPriceHistory`
- OpenAPI + integrator guide + `/docs` Try It

## Frontend

### Nav

Add **Portfolio** link in `layout.tsx` → `/portfolio`.

### Homepage `HoldingsSummary`

Place below Swap History in the ~420px column:

- Disconnected: connect CTA
- Connected: total USD + top 5 non-zero holdings + **View portfolio →**
- Failures non-blocking

### `/portfolio`

- Total value (USD)
- Table: Token | Balance | Price | Value | 24h sparkline (SVG, no chart library)
- `< ~3` history points → show `—`
- Optional MVP+: click row opens `/?` swap with token preselected — nice-to-have, not blocking

## Testing & acceptance

**Backend**

- Unit tests: insert ticks → latest price; history range filter; retention env truncates only when set
- Handler: missing `id`/`ids` → 400; empty history → 200 `[]`
- Sampler: quote failure does not panic (unit or integration light stub)

**Frontend / manual**

- Disconnect empty states
- Connect: balances + valuations when prices available
- Fresh deploy: sparklines `—` until enough samples; after sampling, 24h line appears
- Prices API down: balances still visible; values `—`

**Acceptance**

1. Connected wallet sees holdings valued in USD when routes exist  
2. After sampler runs, history endpoint returns points and sparklines render  
3. Tick DB grows without 14-day auto-delete  
4. No PnL, no external mark dependency for Portfolio, no Limit/DCA  

## File map (implementation hint)

| Area | Likely files |
|------|----------------|
| Price DB + sampler | New module(s) under `crates/api-server` (e.g. `price_store.rs`, `price_sampler.rs`) |
| HTTP | `prices` handlers + `lib.rs` routes |
| Deploy | env docs for `PRICE_DB_PATH`, `PRICE_SAMPLER`, `PRICE_SAMPLE_SECS`, `PRICE_RETENTION_DAYS` |
| SDK | `packages/sdk` |
| UI | `HoldingsSummary.tsx`, `app/portfolio/page.tsx`, sparkline component, `layout.tsx`, `page.tsx` |

## Out of scope follow-ups

- Phase 3: Limit + DCA  
- Richer chart ranges / OHLC aggregation  
- Self-serve price for arbitrary long-tail tokens beyond whitelist (on-demand quote already covers mark; sampling whitelist is the gap)

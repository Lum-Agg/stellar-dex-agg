# Phase 2 Portfolio + Sampled Charts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship quote-valued wallet holdings (homepage summary + `/portfolio`) and self-sampled USDC price ticks with SVG sparklines.

**Architecture:** api-server owns SQLite `price_ticks` + background sampler that marks whitelist tokens via existing `quote_route`. Frontend composes `/balances` + `/prices` + `/prices/history`.

**Tech Stack:** Rust (api-server, rusqlite, QuoteEngine), Next.js frontend, `@lumagg/sdk`.

**Spec:** `docs/superpowers/specs/2026-07-19-phase2-portfolio-charts-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/api-server/src/price_store.rs` | SQLite schema, insert/latest/history/prune |
| `crates/api-server/src/price_mark.rs` | Quote token→USDC (via XLM fallback) |
| `crates/api-server/src/price_sampler.rs` | Periodic whitelist sampling loop |
| `crates/api-server/src/prices.rs` | HTTP `GET /prices` + `/prices/history` |
| `crates/api-server/src/lib.rs` / `state.rs` / `main` path | Wire routes; hold `Option<PriceStore>`; spawn sampler |
| `crates/api-server/Cargo.toml` | Add `rusqlite` (+ tempfile dev) |
| Docs / OpenAPI / SDK | Document + client helpers |
| `packages/frontend/.../Sparkline.tsx` | SVG sparkline |
| `packages/frontend/.../HoldingsSummary.tsx` | Homepage summary |
| `packages/frontend/src/app/portfolio/page.tsx` | Full portfolio |
| `packages/frontend/src/app/layout.tsx` / `page.tsx` | Nav + mount summary |
| Deploy notes | `PRICE_DB_PATH`, sampler envs |

**Well-known constants (mainnet):**
- XLM SAC: `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`
- USDC SAC: `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`
- Decimals: 7 for SAC amounts (`10^7` units = 1 token)

---

### Task 1: `PriceStore` SQLite

**Files:**
- Create: `crates/api-server/src/price_store.rs`
- Modify: `crates/api-server/Cargo.toml` — add `rusqlite` workspace dep (mirror analytics-indexer), `tempfile` dev-dep
- Modify: `crates/api-server/src/lib.rs` — `pub mod price_store;`

- [ ] **Step 1: Write failing unit tests** in `price_store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn insert_and_latest() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 100, 1.5, "usdc").unwrap();
        store.insert_tick("TOK", 200, 1.6, "usdc").unwrap();
        let latest = store.latest("TOK").unwrap().unwrap();
        assert_eq!(latest.ts, 200);
        assert!((latest.price_usdc - 1.6).abs() < 1e-9);
    }

    #[test]
    fn history_range_filter() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 1000, 1.0, "usdc").unwrap();
        store.insert_tick("TOK", 2000, 2.0, "usdc").unwrap();
        store.insert_tick("TOK", 3000, 3.0, "usdc").unwrap();
        let pts = store.history("TOK", 1500, 3000).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].ts, 2000);
        assert_eq!(pts[1].ts, 3000);
    }

    #[test]
    fn prune_older_than() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 100, 1.0, "usdc").unwrap();
        store.insert_tick("TOK", 200, 2.0, "usdc").unwrap();
        let n = store.prune_older_than(150).unwrap();
        assert_eq!(n, 1);
        assert!(store.latest("TOK").unwrap().unwrap().ts == 200);
    }
}
```

- [ ] **Step 2: Run — expect compile fail**

`cargo test -p api-server --lib price_store:: -- --test-threads=1`

- [ ] **Step 3: Implement**

```rust
pub struct PriceTick {
    pub token: String,
    pub ts: i64,
    pub price_usdc: f64,
    pub via: String,
}

pub struct PriceStore { conn: Mutex<Connection> } // or Connection with sync methods on &self via Mutex

impl PriceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> { /* create schema */ }
    pub fn insert_tick(&self, token: &str, ts: i64, price_usdc: f64, via: &str) -> Result<()> { … }
    pub fn latest(&self, token: &str) -> Result<Option<PriceTick>> { … }
    pub fn latest_many(&self, tokens: &[String]) -> Result<Vec<PriceTick>> { … }
    pub fn history(&self, token: &str, from_ts: i64, to_ts: i64) -> Result<Vec<PriceTick>> {
        // ORDER BY ts ASC, WHERE ts >= from AND ts <= to
    }
    pub fn prune_older_than(&self, cutoff_ts: i64) -> Result<usize> { … }
}
```

Schema exactly as spec. Use `Mutex<Connection>` for Sync sharing across axum + sampler.

- [ ] **Step 4: Tests pass**

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(api): add SQLite price tick store

Persist quote-sampled marks for portfolio sparklines.
EOF
)"
```

---

### Task 2: Mark helper + sampler

**Files:**
- Create: `crates/api-server/src/price_mark.rs`
- Create: `crates/api-server/src/price_sampler.rs`
- Modify: `lib.rs` mods

- [ ] **Step 1: Implement `price_mark.rs`**

```rust
pub const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
pub const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
pub const TOKEN_UNITS: u128 = 10_000_000; // 1.0 with 7 decimals

/// Returns (price_usdc per 1 token, via).
pub async fn mark_token_usdc(state: &AppState, token: &str) -> Option<(f64, &'static str)> {
    if token == USDC_SAC { return Some((1.0, "usdc")); }
    if let Some(px) = quote_one(state, token, USDC_SAC).await {
        return Some((px, "usdc"));
    }
    let xlm_out = quote_one(state, token, XLM_SAC).await?;
    let xlm_usdc = quote_one(state, XLM_SAC, USDC_SAC).await?;
    Some((xlm_out * xlm_usdc, "xlm"))
}

async fn quote_one(state: &AppState, token_in: &str, token_out: &str) -> Option<f64> {
    // Build RouteRequest amount_in=TOKEN_UNITS, prefer_soroban=true optional
    // state.quote_route(&req).await
    // if empty sub_orders → None
    // else price = total_expected_out as f64 / TOKEN_UNITS as f64
}
```

Follow `handlers::get_quote` request construction (`TokenId::from_str_auto`, `state.quote_route`).

Unit-test `quote_one` math with a tiny stub only if easy; otherwise test mark USDC constant in a pure test:

```rust
#[test]
fn usdc_is_one() {
    // document constant; integration covered by sampler smoke later
}
```

- [ ] **Step 2: Implement sampler**

```rust
pub fn spawn_price_sampler(state: AppState, store: Arc<PriceStore>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(sample_secs());
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(e) = sample_once(&state, &store).await {
                tracing::warn!(error=%e, "price sample round failed");
            }
            if let Some(days) = retention_days() {
                let cutoff = now - days * 86400;
                let _ = store.prune_older_than(cutoff);
            }
        }
    });
}

async fn sample_once(state: &AppState, store: &PriceStore) -> Result<()> {
    let tokens = sample_token_list(state).await; // priority + top N from engine/token registry
    let ts = unix_now();
    for t in tokens {
        match mark_token_usdc(state, &t).await {
            Some((px, via)) if px.is_finite() && px > 0.0 => {
                let _ = store.insert_tick(&t, ts, px, via);
            }
            _ => tracing::debug!(token=%t, "skip unpriced token"),
        }
    }
    Ok(())
}
```

Env helpers:
- `PRICE_SAMPLER` default on (`!= "0"`)
- `PRICE_SAMPLE_SECS` default `600`
- `PRICE_SAMPLE_TOKEN_LIMIT` default `30`
- `PRICE_RETENTION_DAYS` optional

Token list: start from `dex_adapters::common_balance_tokens` / hardcoded priority (XLM, USDC, EURC, AQUA) then pad from `list_tokens` source used by handler (read how `list_tokens` gets ids — reuse that function or `collect_common_balance_token_ids`).

- [ ] **Step 3: Compile** `cargo test -p api-server --lib price_mark:: price_store::`

- [ ] **Step 4: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(api): quote-based price marks and background sampler

Sample whitelist tokens to USDC ticks for portfolio charts.
EOF
)"
```

---

### Task 3: HTTP `/api/v1/prices` + wire AppState

**Files:**
- Create: `crates/api-server/src/prices.rs`
- Modify: `lib.rs` routes + `handlers` api_root
- Modify: `state.rs` / `run_server` to open `PRICE_DB_PATH`, store `Option<Arc<PriceStore>>`, spawn sampler

- [ ] **Step 1: Handler tests** (with tempfile store injected — prefer `PriceStore` methods + thin handler that takes store from OnceLock for tests, OR test store+mark separately and handler via constructing store env):

Minimum:
1. missing `ids` → 400
2. missing history `id` → 400  
3. history empty → 200 `points: []` when DB open with no rows
4. prices returns inserted latest

Pattern: set `PRICE_DB_PATH` in test like swaps tests; `--test-threads=1`.

- [ ] **Step 2: Implement handlers**

`GET /api/v1/prices?ids=a,b`:
- Parse ids (trim, nonempty, max 50)
- For each: `store.latest` else `mark_token_usdc` + optional `insert_tick`
- Omit failures

`GET /api/v1/prices/history?id=&range=24h|7d`:
- Map range → from_ts
- `store.history`; if no store → 503 or 200 empty (prefer **200 empty** if no DB, **503 only if sampler expected** — MVP: no `PRICE_DB_PATH` → history `[]`, prices still on-demand mark without persist)

- [ ] **Step 3: Wire**

```rust
.route("/api/v1/prices", get(prices::get_prices))
.route("/api/v1/prices/history", get(prices::get_price_history))
```

In `run_server` / `AppState::new`:
```rust
let price_store = std::env::var("PRICE_DB_PATH").ok().filter(|s| !s.is_empty())
    .and_then(|p| PriceStore::open(p).ok().map(Arc::new));
if let Some(store) = price_store.clone() {
    if std::env::var("PRICE_SAMPLER").unwrap_or_else(|_| "1".into()) != "0" {
        price_sampler::spawn_price_sampler(state_for_sampler, store);
    }
}
```

Expose store on `AppState` (cloneable Arc).

- [ ] **Step 4: Tests pass** `cargo test -p api-server --lib prices:: -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(api): add GET /prices and /prices/history

Serve latest marks and sparkline series from sampled ticks.
EOF
)"
```

---

### Task 4: OpenAPI + integrator + deploy env note

**Files:**
- `docs/openapi.yaml`
- `docs/integrator-guide.md` (+ zh-CN)
- Short note in `docs/analytics-indexer.md` or new `docs/portfolio-prices.md` / README deploy section for envs

- [ ] Document endpoints + envs: `PRICE_DB_PATH`, `PRICE_SAMPLER`, `PRICE_SAMPLE_SECS`, `PRICE_SAMPLE_TOKEN_LIMIT`, `PRICE_RETENTION_DAYS`

- [ ] Commit

```bash
git commit -m "$(cat <<'EOF'
docs: document price ticks API and sampler env
EOF
)"
```

---

### Task 5: SDK

**Files:** `packages/sdk/src/index.ts`

```typescript
export interface PriceQuote {
  id: string;
  priceUsdc: number;
  ts: number;
  via: string;
}

export interface PricePoint {
  ts: number;
  priceUsdc: number;
}

async getPrices(ids: string[]): Promise<PriceQuote[]> { … }
async getPriceHistory(id: string, range: '24h' | '7d' = '24h'): Promise<PricePoint[]> { … }
```

- [ ] `npx tsc --noEmit` in packages/sdk
- [ ] Commit `feat(sdk): add getPrices and getPriceHistory`

---

### Task 6: Sparkline + HoldingsSummary + homepage

**Files:**
- Create: `packages/frontend/src/lib/prices.ts`
- Create: `packages/frontend/src/components/Sparkline.tsx`
- Create: `packages/frontend/src/components/HoldingsSummary.tsx`
- Modify: `packages/frontend/src/app/page.tsx`

`prices.ts`:
```typescript
export async function fetchPrices(ids: string[]): Promise<Map<string, number>> …
export async function fetchPriceHistory(id: string, range: '24h'|'7d'): Promise<{ts:number; price_usdc:number}[]> …
```

`Sparkline`: props `points: number[]` or `{ts,price}[]`; width/height ~80×28; stroke zinc/emerald; if `points.length < 3` render `—`

`HoldingsSummary`:
- `useWallet` + `useAccountBalances` + `useTokenList`
- Nonzero balances → fetch prices for those ids
- Show total USD, top 5 rows, link `/portfolio`
- Non-blocking errors

Mount below `SwapHistory` on homepage.

- [ ] `npm run build` in frontend
- [ ] Commit

```bash
git commit -m "$(cat <<'EOF'
feat(frontend): homepage holdings summary with USD marks
EOF
)"
```

---

### Task 7: `/portfolio` page + nav

**Files:**
- Create: `packages/frontend/src/app/portfolio/page.tsx`
- Modify: `packages/frontend/src/app/layout.tsx` — add Portfolio nav link after Swap

Portfolio page:
- Total USD header
- Table all nonzero holdings: symbol, balance, price, value, sparkline (fetch history per visible row or batch top 20 — MVP sequential/parallel with limit)
- Empty / connect states

- [ ] Build passes
- [ ] Commit `feat(frontend): add /portfolio page with sparklines`

---

### Task 8: Docs Try It

**Files:** `packages/frontend/src/app/docs/page.tsx`

Add Endpoint blocks for `/api/v1/prices` and `/api/v1/prices/history` with Try It (ids input; id+range).

- [ ] Commit `docs(frontend): Try It for prices APIs`

---

### Task 9: Verification

- [ ] `cargo test -p api-server --lib price_store:: prices:: -- --test-threads=1`
- [ ] Frontend build
- [ ] Manual checklist from spec (sampler needs `PRICE_DB_PATH` in deploy)
- [ ] Confirm no auto-prune unless `PRICE_RETENTION_DAYS` set

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| price_ticks schema + permanent retention | 1, 2 |
| Sampler 600s, whitelist, USDC/XLM mark | 2 |
| GET /prices + history | 3 |
| OpenAPI / guides / env | 4 |
| SDK | 5 |
| Holdings summary | 6 |
| /portfolio + sparklines | 7 |
| Docs Try It | 8 |
| Non-goals (PnL, Limit, external marks) | not in plan |

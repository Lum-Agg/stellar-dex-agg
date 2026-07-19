# Phase 1 History + Swap UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Titan-style wallet swap history under the homepage SwapCard, 12s quote auto-refresh + manual refresh, and paste-CA / token search — backed by indexer `GET /api/v1/swaps`.

**Architecture:** Extend `analytics-indexer` SQLite with `list_swaps_by_user`; expose it via a thin `api-server` handler mirroring `/stats` (`INDEXER_DB_PATH`). Frontend adds `SwapHistory`, polls/refetches on wallet connect and after swap; `SwapCard` adds a 12s quote poll + refresh button; `TokenSelector` improves paste matching. SDK + OpenAPI + `/docs` stay in sync.

**Tech Stack:** Rust (`analytics-indexer`, `api-server`, rusqlite, axum), Next.js 15 / React 19 (`packages/frontend`), `@lumagg/sdk` (`packages/sdk`).

**Spec:** `docs/superpowers/specs/2026-07-19-phase1-history-swap-ux-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/analytics-indexer/src/store.rs` | Schema index + `list_swaps_by_user`; unit tests |
| `crates/api-server/src/swaps.rs` | `GET /api/v1/swaps` handler (new) |
| `crates/api-server/src/lib.rs` | Register route + `mod swaps` |
| `crates/api-server/src/handlers.rs` | Add `swaps` to `api_root` endpoints map |
| `docs/openapi.yaml` | Document `/api/v1/swaps` |
| `docs/integrator-guide.md` | Short swaps section |
| `packages/sdk/src/index.ts` | `listSwaps` + types |
| `packages/frontend/src/components/SwapHistory.tsx` | History list UI (new) |
| `packages/frontend/src/lib/swaps.ts` | `fetchUserSwaps` helper (new) |
| `packages/frontend/src/app/page.tsx` | Mount `SwapHistory` under SwapCard |
| `packages/frontend/src/components/SwapCard.tsx` | 12s poll, refresh button, emit swap-success event |
| `packages/frontend/src/components/TokenSelector.tsx` | Paste CA / `native` / `CODE:ISSUER` UX |
| `packages/frontend/src/app/docs/page.tsx` | Endpoint + Try It for swaps |

---

### Task 1: Indexer `list_swaps_by_user`

**Files:**
- Modify: `crates/analytics-indexer/src/store.rs`
- Test: same file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests**

Append to `store.rs` (after existing impl):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParsedInvocation, ParsedLeg};
    use tempfile::tempdir;

    fn sample(
        tx_hash: &str,
        user: &str,
        created_at: i64,
        amount_in: i128,
    ) -> StoredInvocation {
        StoredInvocation {
            tx_hash: tx_hash.into(),
            ledger: 1,
            created_at,
            status: "SUCCESS".into(),
            parsed: ParsedInvocation {
                function_name: "swap".into(),
                user_address: user.into(),
                token_in: Some("TOKEN_IN".into()),
                token_out: Some("TOKEN_OUT".into()),
                amount_in,
                amount_out: Some(amount_in + 1),
                is_split: false,
                legs: vec![ParsedLeg {
                    leg_index: 0,
                    dex_source: "soroswap".into(),
                    pool_address: "POOL".into(),
                    token_in: Some("TOKEN_IN".into()),
                    token_out: Some("TOKEN_OUT".into()),
                    amount_in: Some(amount_in),
                }],
            },
        }
    }

    #[test]
    fn list_swaps_by_user_filters_orders_and_limits() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = IndexStore::open(&path).unwrap();
        let u1 = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let u2 = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        store.insert_invocation(&sample("tx_old", u1, 100, 10)).unwrap();
        store.insert_invocation(&sample("tx_new", u1, 200, 20)).unwrap();
        store.insert_invocation(&sample("tx_other", u2, 300, 30)).unwrap();

        let rows = store.list_swaps_by_user(u1, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].tx_hash, "tx_new");
        assert_eq!(rows[1].tx_hash, "tx_old");

        let limited = store.list_swaps_by_user(u1, 1).unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].tx_hash, "tx_new");

        let empty = store.list_swaps_by_user("GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCK3LI", 10).unwrap();
        assert!(empty.is_empty());
    }
}
```

If `tempfile` is not already a dev-dependency of `analytics-indexer`, add it in `crates/analytics-indexer/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p analytics-indexer list_swaps_by_user_filters_orders_and_limits -- --nocapture`

Expected: compile fail — `list_swaps_by_user` not found.

- [ ] **Step 3: Implement schema index + query**

In `init_schema` batch, after existing indexes, add:

```sql
CREATE INDEX IF NOT EXISTS idx_swap_invocations_user_created
  ON swap_invocations(user_address, created_at DESC);
```

Add public types / method on `IndexStore`:

```rust
#[derive(Debug, Clone)]
pub struct UserSwapRow {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub function_name: String,
    pub user_address: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub is_split: bool,
}

impl IndexStore {
    pub fn list_swaps_by_user(&self, user: &str, limit: u32) -> Result<Vec<UserSwapRow>> {
        let limit = limit.clamp(1, 50);
        let mut stmt = self.conn.prepare(
            "SELECT tx_hash, ledger, created_at, status, function_name, user_address,
                    token_in, token_out, amount_in, amount_out, is_split
             FROM swap_invocations
             WHERE user_address = ?1
             ORDER BY created_at DESC, tx_hash DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user, limit], |row| {
            Ok(UserSwapRow {
                tx_hash: row.get(0)?,
                ledger: row.get::<_, i64>(1)? as u32,
                created_at: row.get(2)?,
                status: row.get(3)?,
                function_name: row.get(4)?,
                user_address: row.get(5)?,
                token_in: row.get(6)?,
                token_out: row.get(7)?,
                amount_in: row.get(8)?,
                amount_out: row.get(9)?,
                is_split: row.get::<_, i32>(10)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}
```

Re-export if useful: in `lib.rs` leave as `store::UserSwapRow` (api-server will import via `analytics_indexer::store::{IndexStore, UserSwapRow}` — ensure `store` module items used are `pub`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p analytics-indexer list_swaps_by_user -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/analytics-indexer/src/store.rs crates/analytics-indexer/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(indexer): list swaps by user for history API

Add indexed query used by GET /api/v1/swaps for wallet-scoped history.
EOF
)"
```

---

### Task 2: `GET /api/v1/swaps` on api-server

**Files:**
- Create: `crates/api-server/src/swaps.rs`
- Modify: `crates/api-server/src/lib.rs`
- Modify: `crates/api-server/src/handlers.rs` (`api_root` endpoints)
- Test: `crates/api-server/src/swaps.rs` `#[cfg(test)]` (or `tests/swaps_api.rs` if preferred)

- [ ] **Step 1: Write failing HTTP-oriented unit tests in `swaps.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use analytics_indexer::{
        parser::{ParsedInvocation, ParsedLeg},
        store::{IndexStore, StoredInvocation},
    };
    use tempfile::tempdir;

    fn seed_db(path: &std::path::Path) {
        let store = IndexStore::open(path).unwrap();
        let _ = store.insert_invocation(&StoredInvocation {
            tx_hash: "abc".into(),
            ledger: 10,
            created_at: 1_700_000_000,
            status: "SUCCESS".into(),
            parsed: ParsedInvocation {
                function_name: "swap".into(),
                user_address: "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY".into(),
                token_in: Some("TIN".into()),
                token_out: Some("TOUT".into()),
                amount_in: 1_000_0000,
                amount_out: Some(2_000_0000),
                is_split: false,
                legs: vec![],
            },
        });
    }

    #[tokio::test]
    async fn missing_user_is_400() {
        let resp = get_swaps(Query(SwapsQuery { user: None, limit: None })).await.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_user_is_400() {
        let resp = get_swaps(Query(SwapsQuery {
            user: Some("not-an-address".into()),
            limit: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn no_db_env_is_503() {
        std::env::remove_var("INDEXER_DB_PATH");
        std::env::remove_var("LUMAGG_INDEXER_DB_PATH");
        let resp = get_swaps(Query(SwapsQuery {
            user: Some("GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY".into()),
            limit: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn returns_rows_when_db_configured() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("idx.db");
        seed_db(&path);
        std::env::set_var("INDEXER_DB_PATH", path.to_str().unwrap());
        let resp = get_swaps(Query(SwapsQuery {
            user: Some("GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY".into()),
            limit: Some(20),
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        std::env::remove_var("INDEXER_DB_PATH");
    }
}
```

Add `tempfile` to `api-server` dev-dependencies if missing. Note: `into_response()` requires importing `axum::response::IntoResponse`. Env mutation in parallel tests can race — run this file’s tests serially if flaky (`--test-threads=1` for this package’s swaps tests) or use a scoped env helper; prefer documenting `cargo test -p api-server swaps::tests -- --test-threads=1`.

- [ ] **Step 2: Run tests — expect fail (module missing)**

Run: `cargo test -p api-server swaps::tests -- --test-threads=1`

Expected: fail to compile.

- [ ] **Step 3: Implement `swaps.rs`**

Mirror `stats.rs` DB path helper (duplicate small private fn or share later — YAGNI: duplicate 5 lines):

```rust
//! Wallet-scoped swap history from analytics-indexer SQLite.

use {
    analytics_indexer::store::IndexStore,
    axum::{
        extract::Query,
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Deserialize)]
pub struct SwapsQuery {
    pub user: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SwapsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SwapsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SwapsData {
    pub swaps: Vec<SwapItem>,
}

#[derive(Debug, Serialize)]
pub struct SwapItem {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub function_name: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub is_split: bool,
}

fn indexer_db_path() -> Option<String> {
    std::env::var("INDEXER_DB_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("LUMAGG_INDEXER_DB_PATH").ok().filter(|s| !s.is_empty()))
}

fn looks_like_g_address(s: &str) -> bool {
    s.len() == 56 && s.starts_with('G') && s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub async fn get_swaps(Query(params): Query<SwapsQuery>) -> Response {
    let Some(user) = params.user.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some("missing required query param: user".into()),
            }),
        )
            .into_response();
    };
    if !looks_like_g_address(user) {
        return (
            StatusCode::BAD_REQUEST,
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some("user must be a Stellar G... address".into()),
            }),
        )
            .into_response();
    }

    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some("Analytics DB not configured (set INDEXER_DB_PATH on api-server)".into()),
            }),
        )
            .into_response();
    };

    let store = match IndexStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SwapsResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    match store.list_swaps_by_user(user, limit) {
        Ok(rows) => {
            let swaps = rows
                .into_iter()
                .map(|r| SwapItem {
                    tx_hash: r.tx_hash,
                    ledger: r.ledger,
                    created_at: r.created_at,
                    status: r.status,
                    function_name: r.function_name,
                    token_in: r.token_in,
                    token_out: r.token_out,
                    amount_in: r.amount_in,
                    amount_out: r.amount_out,
                    is_split: r.is_split,
                })
                .collect();
            (
                StatusCode::OK,
                Json(SwapsResponse {
                    success: true,
                    data: Some(SwapsData { swaps }),
                    error: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}
```

Wire in `lib.rs`:

```rust
pub mod swaps;
// ...
.route("/api/v1/swaps", get(swaps::get_swaps))
```

In `handlers.rs` `api_root` endpoints map add: `"swaps": "/api/v1/swaps"`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p api-server swaps::tests -- --test-threads=1`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/api-server/src/swaps.rs crates/api-server/src/lib.rs crates/api-server/src/handlers.rs crates/api-server/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(api): add GET /api/v1/swaps for wallet history

Expose indexer swap_invocations filtered by user for the retail UI.
EOF
)"
```

---

### Task 3: OpenAPI + integrator guide

**Files:**
- Modify: `docs/openapi.yaml`
- Modify: `docs/integrator-guide.md` (and `docs/integrator-guide.zh-CN.md` if keeping parity — add a short mirror section)

- [ ] **Step 1: Add `/api/v1/swaps` path to OpenAPI**

After `/api/v1/stats` block, add path with query `user` (required), `limit` (optional 1–50), responses 200/400/503, schema `SwapsResponse` / `SwapItem` matching Task 2 fields (snake_case JSON).

- [ ] **Step 2: Document in integrator guide**

Add under stats section:

```markdown
### Wallet swap history

```bash
curl -s "https://api.lumagg.xyz/api/v1/swaps?user=G...&limit=20" | jq .
```

Returns recent aggregator invocations for that account (same SQLite as `/stats`).
```

- [ ] **Step 3: Commit**

```bash
git add docs/openapi.yaml docs/integrator-guide.md docs/integrator-guide.zh-CN.md
git commit -m "$(cat <<'EOF'
docs: document GET /api/v1/swaps

OpenAPI + integrator guide for wallet-scoped history.
EOF
)"
```

---

### Task 4: SDK `listSwaps`

**Files:**
- Modify: `packages/sdk/src/index.ts`

- [ ] **Step 1: Add types + method**

```typescript
export interface SwapRecord {
  txHash: string;
  ledger: number;
  createdAt: number;
  status: string;
  functionName: string;
  tokenIn?: string;
  tokenOut?: string;
  amountIn: string;
  amountOut?: string;
  isSplit: boolean;
}

export interface ListSwapsParams {
  user: string;
  limit?: number;
}

// inside LumAggClient:
async listSwaps(params: ListSwapsParams): Promise<SwapRecord[]> {
  const search = new URLSearchParams({ user: params.user });
  if (params.limit !== undefined) search.set('limit', String(params.limit));
  const resp = await fetch(`${this.baseUrl}/api/v1/swaps?${search}`, {
    headers: this.headers(),
  });
  const json = await resp.json();
  if (!json.success) throw new Error(json.error || 'listSwaps failed');
  return (json.data?.swaps || []).map((r: Record<string, unknown>) => ({
    txHash: String(r.tx_hash ?? ''),
    ledger: Number(r.ledger ?? 0),
    createdAt: Number(r.created_at ?? 0),
    status: String(r.status ?? ''),
    functionName: String(r.function_name ?? ''),
    tokenIn: r.token_in != null ? String(r.token_in) : undefined,
    tokenOut: r.token_out != null ? String(r.token_out) : undefined,
    amountIn: String(r.amount_in ?? '0'),
    amountOut: r.amount_out != null ? String(r.amount_out) : undefined,
    isSplit: Boolean(r.is_split),
  }));
}
```

- [ ] **Step 2: Typecheck**

Run: `cd packages/sdk && npx tsc --noEmit` (or package script if present)

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add packages/sdk/src/index.ts
git commit -m "$(cat <<'EOF'
feat(sdk): add listSwaps for wallet history

Wrap GET /api/v1/swaps for integrators and the frontend.
EOF
)"
```

---

### Task 5: Frontend fetch helper + `SwapHistory`

**Files:**
- Create: `packages/frontend/src/lib/swaps.ts`
- Create: `packages/frontend/src/components/SwapHistory.tsx`
- Modify: `packages/frontend/src/app/page.tsx`

- [ ] **Step 1: Add `swaps.ts`**

```typescript
const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

export type UserSwap = {
  tx_hash: string;
  ledger: number;
  created_at: number;
  status: string;
  function_name: string;
  token_in: string | null;
  token_out: string | null;
  amount_in: string;
  amount_out: string | null;
  is_split: boolean;
};

export async function fetchUserSwaps(user: string, limit = 20): Promise<UserSwap[]> {
  const qs = new URLSearchParams({ user, limit: String(limit) });
  const resp = await fetch(`${API_URL}/api/v1/swaps?${qs}`);
  const json = await resp.json();
  if (!resp.ok || !json.success) {
    throw new Error(json.error || `swaps HTTP ${resp.status}`);
  }
  return json.data?.swaps ?? [];
}

/** Dispatched by SwapCard after a successful on-chain swap. */
export const SWAP_SUCCESS_EVENT = 'lumagg:swap-success';
```

- [ ] **Step 2: Implement `SwapHistory.tsx`**

Behavior:
- `useWallet()` for `address`
- If no address: muted “Connect wallet to see your swaps”
- If address: fetch on mount / address change; listen for `SWAP_SUCCESS_EVENT` and refetch after ~2s (indexer lag)
- Render up to 20 rows: relative time (`formatDistance`-style simple helper), truncated symbols via `useTokenList` + `displayTokenSymbol`, amounts with 7 decimals default, status, link `https://stellar.expert/explorer/public/tx/${tx_hash}`
- Error/503: one-line “History unavailable” — do not throw to parent

Sketch structure:

```tsx
'use client';

import { useCallback, useEffect, useState } from 'react';
import { useWallet } from '@/lib/wallet-context';
import { useTokenList } from './TokenSelector';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { fetchUserSwaps, SWAP_SUCCESS_EVENT, type UserSwap } from '@/lib/swaps';
import { formatBalanceDisplay } from '@/lib/balance';

function relativeTime(ts: number): string {
  const sec = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (sec < 60) return `${sec}s ago`;
  if (sec < 3600) return `${Math.floor(sec / 60)}m ago`;
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ago`;
  return `${Math.floor(sec / 86400)}d ago`;
}

export function SwapHistory() {
  const { address, connect, connecting } = useWallet();
  const tokens = useTokenList();
  const [swaps, setSwaps] = useState<UserSwap[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    if (!address) return;
    setLoading(true);
    setError(null);
    try {
      setSwaps(await fetchUserSwaps(address, 20));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'History unavailable');
    } finally {
      setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const onSuccess = () => {
      window.setTimeout(() => void load(), 2000);
    };
    window.addEventListener(SWAP_SUCCESS_EVENT, onSuccess);
    return () => window.removeEventListener(SWAP_SUCCESS_EVENT, onSuccess);
  }, [load]);

  // ... render list matching SwapCard width / zinc styling
}
```

Match existing Tailwind: zinc borders, compact rows, no card chrome beyond a light top border section titled “Swap history”.

- [ ] **Step 3: Mount on homepage**

In `page.tsx`, inside the first centered section after the tagline `<p className="mt-4 ...">`:

```tsx
import { SwapHistory } from '@/components/SwapHistory';
// ...
<SwapCard />
<p className="mt-4 ...">...</p>
<div className="mt-6 w-full">
  <SwapHistory />
</div>
```

- [ ] **Step 4: Manual smoke**

Run frontend locally against prod or staging API with `INDEXER_DB_PATH` set; connect a wallet that has indexed swaps.

- [ ] **Step 5: Commit**

```bash
git add packages/frontend/src/lib/swaps.ts packages/frontend/src/components/SwapHistory.tsx packages/frontend/src/app/page.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): show wallet swap history under SwapCard

Pull GET /api/v1/swaps and refetch after successful swaps.
EOF
)"
```

---

### Task 6: Quote auto-refresh + manual refresh

**Files:**
- Modify: `packages/frontend/src/components/SwapCard.tsx`

- [ ] **Step 1: Extract quote fetch into `loadQuote`**

Refactor the debounced `useEffect` body into a `useCallback` `loadQuote(opts?: { silent?: boolean })` that:
- Builds stroops from `amountIn` / `tokenIn`
- Calls `getQuote`
- On success: `setQuote`
- On failure: if `silent`, **keep** previous quote and optionally set a soft error; if not silent (user typed amount), clear quote as today

- [ ] **Step 2: Keep debounce on amount/token/slippage change**

Debounced 500ms call to `loadQuote()` (non-silent) — same deps as today.

- [ ] **Step 3: Add 12s interval**

```tsx
useEffect(() => {
  if (!amountIn || parseFloat(amountIn) <= 0) return;
  const id = window.setInterval(() => {
    void loadQuote({ silent: true });
  }, 12_000);
  return () => window.clearInterval(id);
}, [amountIn, tokenIn.id, tokenOut.id, slippage, loadQuote]);
```

- [ ] **Step 4: Manual refresh control**

When `quote` is shown (rate row / route header area), add a small button:

```tsx
<button
  type="button"
  aria-label="Refresh quote"
  disabled={loading}
  onClick={() => void loadQuote({ silent: true })}
  className="text-zinc-500 hover:text-zinc-300 text-xs"
>
  Refresh
</button>
```

- [ ] **Step 5: Emit success event after confirmed swap**

Where `txResult` / hash is set on success, also:

```tsx
import { SWAP_SUCCESS_EVENT } from '@/lib/swaps';
window.dispatchEvent(new Event(SWAP_SUCCESS_EVENT));
```

- [ ] **Step 6: Commit**

```bash
git add packages/frontend/src/components/SwapCard.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): auto-refresh quotes every 12s

Silent poll plus manual refresh; notify history after swap success.
EOF
)"
```

---

### Task 7: TokenSelector paste CA polish

**Files:**
- Modify: `packages/frontend/src/components/TokenSelector.tsx`

- [ ] **Step 1: Improve filter + empty state**

Update search placeholder to: `Search name, C…, or CODE:ISSUER`.

Normalize search:
- trim
- if search equals `native` (case-insensitive), also match `NATIVE_CONTRACT` / XLM priority id
- if search contains `:`, also match `id` case-insensitively and symbol+issuer patterns already in `id`

```tsx
const q = search.trim().toLowerCase();
const filtered = tokens.filter((t) => {
  if (t.id === exclude) return false;
  if (!q) return true;
  if (t.symbol.toLowerCase().includes(q) || t.name.toLowerCase().includes(q)) return true;
  if (t.id.toLowerCase().includes(q)) return true;
  if (q === 'native' && (t.symbol === 'XLM' || t.id === NATIVE_CONTRACT)) return true;
  return false;
});
```

Import `NATIVE_CONTRACT` from `@/lib/tokenDisplay`.

When `q.length >= 4` and `filtered.length === 0`, show: `Token not in list` (no resolve API in MVP).

- [ ] **Step 2: Optional auto-select on exact id match**

If exactly one filter result and `q === filtered[0].id.toLowerCase()`, show a primary row “Select {symbol}” — or on Enter key select it. Keep UX minimal: exact full-id match button is enough.

- [ ] **Step 3: Commit**

```bash
git add packages/frontend/src/components/TokenSelector.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): improve token paste / search matching

Support native alias and clearer not-in-list empty state.
EOF
)"
```

---

### Task 8: Docs page Try It

**Files:**
- Modify: `packages/frontend/src/app/docs/page.tsx`

- [ ] **Step 1: Add Endpoint block**

After tokens (or after build_tx), insert:

```tsx
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
```

Implement `SwapsTryIt` like `PingTryIt` but with an input defaulting to `DEMO_USER` and fetching `/api/v1/swaps?user=…`.

- [ ] **Step 2: Commit**

```bash
git add packages/frontend/src/app/docs/page.tsx
git commit -m "$(cat <<'EOF'
docs(frontend): add /api/v1/swaps Try It on docs page
EOF
)"
```

---

### Task 9: Verification checklist

- [ ] **Step 1: Backend tests**

```bash
cargo test -p analytics-indexer list_swaps_by_user
cargo test -p api-server swaps::tests -- --test-threads=1
```

Expected: all PASS.

- [ ] **Step 2: Manual acceptance (against env with `INDEXER_DB_PATH`)**

1. Homepage disconnected → history empty-state  
2. Connect wallet with known swaps → rows + Expert links  
3. Enter amount → quote updates; wait ~12s → quote refreshes without clearing  
4. Click Refresh → immediate requote  
5. Paste known SAC id in token search → token appears / selectable  
6. Stop indexer DB path on a local API → history shows unavailable; swap still works  

- [ ] **Step 3: Final commit only if docs/scripts leftover**

No empty commit. If OpenAPI/docs already committed in Task 3/8, skip.

---

## Spec coverage (self-review)

| Spec item | Task |
|-----------|------|
| `list_swaps_by_user` + user index | Task 1 |
| `GET /api/v1/swaps` 400/503/200 | Task 2 |
| OpenAPI / integrator docs | Task 3 |
| SDK `listSwaps` | Task 4 |
| History under SwapCard | Task 5 |
| Quote 12s + manual refresh | Task 6 |
| Paste CA / search | Task 7 |
| `/docs` Try It | Task 8 |
| Acceptance / non-blocking history | Tasks 5–6, 9 |
| Non-goals (limit/DCA/exact-out/venue) | Not in any task |

**Deferred by design:** `tokens/resolve`, cursor pagination, dedicated `/history` page.

# LumAgg Phase 1: Swap History + Swap UX Polish

**Date:** 2026-07-19  
**Status:** Approved for planning  
**Inspired by:** jup.ag / titan.exchange retail surfaces

## Goal

Make the LumAgg homepage feel closer to Titan/Jupiter for **market swaps only**: wallet-scoped recent swap history under the swap card, quote auto-refresh, and paste-CA / token search — without building Limit, DCA, Portfolio, or Charts yet.

## Non-goals (explicit)

- Limit orders, DCA / recurring, TWAP
- Exact-out quotes, venue include/exclude, route preference knobs
- Dedicated `/history` page, filters, CSV export of personal history
- Portfolio / PnL / price charts / token detail pages
- Non–LumAgg-aggregator trades (direct DEX or Classic-only fills)
- WebSocket quote streaming

These are deferred to Phase 2 (Portfolio + Charts) and Phase 3 (Limit + DCA) per product roadmap.

## Context

- Retail surface: Next.js app at `packages/frontend` (`/` = SwapCard; `/docs`, `/stats`).
- Indexer already stores `swap_invocations.user_address` in SQLite (`analytics-indexer`); public API today only exposes rollups via `GET /api/v1/stats`.
- Quote path: `GET /api/v1/quote` → `POST /api/v1/build_tx`; tokens via `GET /api/v1/tokens`.

## Product decisions (locked)

| Decision | Choice |
|----------|--------|
| Roadmap order | Phase 1 History+UX → Phase 2 Portfolio+Charts → Phase 3 Limit+DCA |
| History placement | Below SwapCard on homepage (Titan-style), not a separate page |
| Phase 1 UX depth | Lean MVP: refresh + paste CA + history list |
| History data source | Indexer-backed API (same DB as `/stats`) |

## Architecture

```
Wallet connect
     │
     ▼
Frontend SwapHistory ──GET /api/v1/swaps?user=G…──► api-server
                                                         │
                                                         ▼
                                              analytics-indexer SQLite
                                              (INDEXER_DB_PATH)

SwapCard quote loop ──GET /api/v1/quote──► (existing)

TokenSelector paste ──match /api/v1/tokens──► optional resolve later
```

Single source of truth for personal history: all `swap_invocations` rows for `user_address` (includes `swap` and any `round_trip_swap`). MVP UI lists them the same way; no special arb filtering.

## API

### `GET /api/v1/swaps`

**Query**

| Param | Required | Notes |
|-------|----------|--------|
| `user` | yes | Stellar account `G…` (reject invalid format with 400) |
| `limit` | no | Default 20, max 50 |
| `cursor` | no | **MVP: omit** — only `limit`. Add keyset cursor in a follow-up if lists grow. |

**Success (200)**

```json
{
  "success": true,
  "data": {
    "swaps": [
      {
        "tx_hash": "…",
        "ledger": 0,
        "created_at": 0,
        "status": "SUCCESS",
        "function_name": "swap",
        "token_in": "…",
        "token_out": "…",
        "amount_in": "…",
        "amount_out": "…",
        "is_split": false
      }
    ]
  }
}
```

`next_cursor` is reserved for later; MVP responses omit it or always send `null`.

**Errors**

- Missing/invalid `user` → 400
- `INDEXER_DB_PATH` unset / DB open failure → 503 with message (same pattern as `/stats`)
- Empty history → 200 with `swaps: []`

**Store changes (`analytics-indexer`)**

- Add `list_swaps_by_user(user, limit)` querying `swap_invocations` ordered by `created_at DESC, tx_hash DESC`
- Add index: `CREATE INDEX IF NOT EXISTS idx_swap_invocations_user_created ON swap_invocations(user_address, created_at DESC)`

**Wiring**

- New handler module or extend `stats.rs` patterns in `api-server`
- Route in `lib.rs`: `.route("/api/v1/swaps", get(…))`
- Env: reuse `INDEXER_DB_PATH` / `LUMAGG_INDEXER_DB_PATH`

### Token paste / resolve

- **MVP required:** frontend matches paste against `GET /api/v1/tokens` by contract id, `native`, or `CODE:ISSUER` string already used in the app
- **Optional same iteration:** `GET /api/v1/tokens/resolve?id=…` for unknown SAC metadata; if skipped, show “Token not in list”

### SDK / docs

- `@lumagg/sdk`: `listSwaps({ user, limit })`
- `/docs` Try It entry for `/api/v1/swaps`

## Frontend

### Layout (`page.tsx`)

Order in the centered ~420px column:

1. Disclaimer  
2. `SwapCard` (refresh affordance inside)  
3. Short tagline (existing)  
4. **`SwapHistory`** (new)  
5. Rest of marketing sections unchanged  

### `SwapHistory`

- Connected: fetch swaps for wallet public key; show relative time, in→out amounts (resolve decimals/symbols via token list when possible; fallback to truncated contract ids), status, StellarExpert link
- Disconnected: prompt to connect
- After successful swap: refetch shortly (or optimistic row)
- Failures/503: non-blocking message; never disable SwapCard

### Quote refresh (`SwapCard`)

- When `amountIn` + tokens valid: poll quote every **12 seconds**
- Manual refresh control next to quote
- Token/amount change: immediate requote + reset timer
- Failed refresh: keep last good quote

### Paste CA (`TokenSelector`)

- Accept `C…`, `native`, `CODE:ISSUER` in search
- Select on match; otherwise clear messaging

## Testing & acceptance

**Automated**

- Indexer unit tests for `list_swaps_by_user` (multi-user isolation, limit, empty)
- API tests: 400 without user; 503 without DB path; 200 with fixture DB when feasible

**Manual**

- Disconnect → empty-state copy  
- Connect address with known indexed swaps → list renders + Expert links  
- Enter amount → quote refreshes ~12s; button refreshes immediately  
- Paste known token id → selection works  

**Acceptance**

1. After a successful LumAgg aggregator swap, that wallet sees the row under History once indexer has ingested it  
2. History outage does not break swap  
3. No Exact-out / venue filter / Limit / DCA shipped in this phase  

## File map (implementation hint)

| Area | Likely files |
|------|----------------|
| Indexer query + index | `crates/analytics-indexer/src/store.rs` |
| HTTP API | `crates/api-server/src/lib.rs`, new or extended swaps handler, mirror `stats.rs` |
| SDK | `packages/sdk` (`@lumagg/sdk`) |
| UI | `packages/frontend/src/app/page.tsx`, `SwapCard.tsx`, `TokenSelector.tsx`, new `SwapHistory.tsx` |
| Docs page | `packages/frontend/src/app/docs/…` |

## Out of scope follow-ups (do not implement in Phase 1 plan tasks)

- Phase 2: Portfolio balances page + charts  
- Phase 3: Limit + DCA execution stack  
- Exact-out and venue filters as a small Phase 1.1 if needed after MVP ships  

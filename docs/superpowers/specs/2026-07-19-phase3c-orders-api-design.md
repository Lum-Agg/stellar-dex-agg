# LumAgg Phase 3c: Limit Orders API + Event Index (Lean)

**Date:** 2026-07-19  
**Status:** Approved for planning  
**Depends on:** order-escrow events (3b), api-server prepare/sim patterns, analytics-indexer getEvents

## Goal

Index escrow lifecycle events into SQLite and expose retail-friendly **list + build unsigned XDR** endpoints for create/cancel Limit orders (wallet signs). No UI, no DCA, no server-side user keys.

## Decisions

| Topic | Choice |
|-------|--------|
| Write path | `build_create` / `build_cancel` → unsigned XDR (like `build_tx`) |
| Read path | `GET /orders?user=` from indexer DB |
| Storage | Extend analytics SQLite **or** `ORDER_DB_PATH` (prefer same file as indexer via `INDEXER_DB_PATH` with new tables) |
| Ingest | Extend analytics-indexer (or thin sibling poller) for escrow contract events |
| DCA / UI | Out of scope |

## Data model

```sql
CREATE TABLE IF NOT EXISTS limit_orders (
  order_id INTEGER PRIMARY KEY,
  owner TEXT NOT NULL,
  token_in TEXT NOT NULL,
  token_out TEXT NOT NULL,
  amount_in_initial TEXT,          -- from created event
  amount_in_remaining TEXT NOT NULL,
  limit_out_per_in_e7 TEXT NOT NULL,
  expires_ledger INTEGER NOT NULL,
  status TEXT NOT NULL,            -- open|filled|cancelled|expired
  created_ledger INTEGER,
  updated_ledger INTEGER NOT NULL,
  created_at INTEGER,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_limit_orders_owner ON limit_orders(owner, status);
```

Apply events:
- `order_created` → insert open  
- `order_filled` → update remaining; filled if 0  
- `order_cancelled` / `order_expired` → status + remaining 0  

## API

| Method | Path | Behavior |
|--------|------|----------|
| `GET` | `/api/v1/orders?user=G…&status=open\|all` | List from DB; 400 bad user; 503 no DB |
| `POST` | `/api/v1/orders/build_create` | Body: user, tokens, amount_in, limit_out_per_in_e7, expires_ledger → simulate+prepared unsigned XDR invoking `create_limit` |
| `POST` | `/api/v1/orders/build_cancel` | Body: user, order_id → unsigned XDR for `cancel` |

Response style matches `build_tx` (`success`, `unsigned_tx_xdr`, fee fields).

Optional: `GET /api/v1/orders/{id}` — nice-to-have, not required for MVP.

## Ingest

- Config: `ESCROW_CONTRACT` (+ existing RPC)  
- Mode: poll getEvents for escrow topics (reuse dex_adapters helpers)  
- Can live in `analytics-indexer` with dual contract filters, or `limit-keeper` write path — **prefer analytics-indexer** so API shares `INDEXER_DB_PATH`

## Non-goals

- Frontend Limit tab (3d)  
- DCA  
- Keeper changes beyond consuming same events  
- Server holding user secrets  
- Mainnet deploy automation  

## Testing & acceptance

- Store unit: apply event sequence → statuses  
- API: list filters; build_create missing fields → 400  
- Manual: build_create → wallet sign → event indexed → GET lists open order  

## Acceptance

1. After on-chain create, list shows the order once indexer catches up  
2. build_cancel produces signable XDR for owner  
3. No UI shipped  

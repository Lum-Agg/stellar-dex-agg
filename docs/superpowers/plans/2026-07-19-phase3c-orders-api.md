# Phase 3c — Limit Orders API + Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist escrow order events in SQLite and serve `GET /orders` plus `build_create` / `build_cancel` unsigned XDR endpoints.

**Architecture:** Extend `analytics-indexer` to ingest escrow events into `limit_orders`; `api-server` reads DB and builds Soroban invoke XDRs via existing prepare/sim utilities.

**Tech Stack:** Rust, rusqlite, dex_adapters RPC events, api-server axum + soroban prepare (mirror `build_tx` / swaps handlers).

**Spec:** `docs/superpowers/specs/2026-07-19-phase3c-orders-api-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/analytics-indexer/src/store.rs` | `limit_orders` schema + upsert helpers |
| `crates/analytics-indexer/src/order_events.rs` (new) | Parse escrow lifecycle events |
| `crates/analytics-indexer/src/ingest.rs` / `config.rs` | Poll escrow contract when `ESCROW_CONTRACT` set |
| `crates/api-server/src/orders.rs` (new) | HTTP handlers |
| `crates/api-server/src/lib.rs` | Routes |
| `packages/sdk` | `listOrders`, `buildCreateOrder`, `buildCancelOrder` (Task 5) |
| Docs OpenAPI / integrator | Task 6 |

---

### Task 1: `limit_orders` store

- Schema + `upsert_created`, `apply_filled`, `apply_closed(status)`, `list_by_owner(owner, status_filter)`
- Unit tests with tempfile
- Commit: `feat(indexer): limit_orders table and queries`

### Task 2: Parse + ingest escrow events

- Parse `order_created` / `order_filled` / `order_cancelled` / `order_expired` (match contract payloads)
- Config `ESCROW_CONTRACT`; ingest loop fetches events (can share cursor table with type discriminator or separate `escrow_cursor`)
- Unit tests for parser
- Commit: `feat(indexer): ingest escrow order lifecycle events`

### Task 3: `GET /api/v1/orders`

- Query `user` required; `status` optional `open|all` default open
- Mirror swaps handler DB path / validation
- Tests: 400 / 503 / 200 list
- Commit: `feat(api): GET /api/v1/orders`

### Task 4: `build_create` + `build_cancel`

- POST JSON bodies; build `invokeHostFunction` for escrow `create_limit` / `cancel`
- Reuse `prepare_transaction_xdr` / simulate patterns from `handlers::build_tx` and arbitrage prepare
- Auth: source account = user; operations require user signature
- Return unsigned XDR
- Unit/integration lite: reject bad body; optional sim smoke if fixtures allow
- Commit: `feat(api): build_create and build_cancel for limit orders`

### Task 5: SDK + OpenAPI brief

- SDK methods + regenerate `packages/sdk/dist`
- OpenAPI paths + short integrator note
- Commit: `feat(sdk): limit order list and build helpers` (+ docs commit if cleaner split)

### Task 6: Verify

- `cargo test -p analytics-indexer` relevant + `cargo test -p api-server --lib orders::`
- Confirm no frontend Limit UI

---

## Out of scope

- 3d UI, DCA, keeper changes, deploy scripts

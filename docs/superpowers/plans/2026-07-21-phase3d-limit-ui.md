# Phase 3d Limit Orders UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Testnet-only Limit panel on homepage: create, list open, cancel (wallet signs XDR).

**Architecture:** `orderType` state on `/`; `LimitCard` + `OpenOrders`; Limit API via `NEXT_PUBLIC_LIMIT_API_URL`; `signTx` accepts optional testnet passphrase.

**Tech Stack:** Next.js 15, existing wallet kit, Horizon testnet submit

**Spec:** `docs/superpowers/specs/2026-07-21-phase3d-limit-ui-design.md`

---

### Task 1: Wallet signTx network override

**Files:** `packages/frontend/src/lib/wallet-context.tsx`

- [x] `signTx(xdr, opts?: { networkPassphrase?: string })` — default PUBLIC

### Task 2: Limit API client + helpers

**Files:** Create `packages/frontend/src/lib/limit-orders.ts`

- [x] `LIMIT_API_URL`, list/create/cancel, price e7 helpers, fetch latest ledger from Horizon testnet

### Task 3: OrderTypeRail selectable

**Files:** `packages/frontend/src/components/OrderTypeRail.tsx`

- [x] Enable Limit; `onSelect(id)` + `active`

### Task 4: LimitCard + OpenOrders

**Files:**
- Create `packages/frontend/src/components/LimitCard.tsx`
- Create `packages/frontend/src/components/OpenOrders.tsx`

- [x] LimitCard create form + OpenOrders list/cancel

### Task 5: Wire homepage

**Files:** `packages/frontend/src/app/page.tsx`

- [x] Toggle Instant vs Limit column

### Task 6: Verify

- [x] `npm run lint` + `tsc --noEmit`

# Titan Portfolio Restyle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle `/portfolio` into a Titan-leaning profile page with hero, tabs (Holdings + Swap history + Soon tabs), and embedded sparklines.

**Architecture:** Extract `useSwapHistory` hook; add `portfolio/*` presentational components; thin `portfolio/page.tsx` orchestrates data; `SwapHistory` supports `compact` | `profile` variants.

**Tech Stack:** Next.js App Router, React, existing prices/balances/swaps APIs

---

### Task 1: Extract `useSwapHistory` hook

**Files:**
- Create: `packages/frontend/src/lib/useSwapHistory.ts`
- Modify: `packages/frontend/src/components/SwapHistory.tsx`

- [ ] Move fetch/refetch/event logic into hook; SwapHistory consumes hook

### Task 2: SwapHistory variants

**Files:**
- Modify: `packages/frontend/src/components/SwapHistory.tsx`

- [ ] `variant="compact"` — home Activity (default)
- [ ] `variant="profile"` — full-width history panel for portfolio tab

### Task 3: Portfolio components

**Files:**
- Create: `packages/frontend/src/components/portfolio/ProfileHero.tsx`
- Create: `packages/frontend/src/components/portfolio/ProfileTabs.tsx`
- Create: `packages/frontend/src/components/portfolio/HoldingsTable.tsx`
- Modify: `packages/frontend/src/components/TokenSelector.tsx` — export `TokenIcon`

### Task 4: Portfolio page orchestration

**Files:**
- Modify: `packages/frontend/src/app/portfolio/page.tsx`

- [ ] Hero + tabs + holdings/history panels; disconnected CTA

### Task 5: Verify

- [ ] `npx tsc --noEmit` in `packages/frontend`

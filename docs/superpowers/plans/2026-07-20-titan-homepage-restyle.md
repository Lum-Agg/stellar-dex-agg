# Titan-leaning Homepage Restyle — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle LumAgg shell + homepage into a Titan-leaning charcoal/mint swap desk; remove marketing sections; keep swap logic.

**Architecture:** In-place CSS token swap + component chrome updates in `packages/frontend`. Homepage becomes nav + SwapCard + slim history.

**Tech Stack:** Next.js app router, Tailwind v4 (`@import "tailwindcss"`), existing React swap components.

**Spec:** `docs/superpowers/specs/2026-07-20-titan-homepage-restyle-design.md`

---

## File map

| File | Change |
|------|--------|
| `packages/frontend/src/app/globals.css` | New charcoal/mint tokens; kill glow gradients |
| `packages/frontend/src/app/layout.tsx` | Nav / shell chrome |
| `packages/frontend/src/app/page.tsx` | Strip Steps/Compare/Venues/FAQ; slim holdings |
| `packages/frontend/src/components/SwapCard.tsx` | Titan-like chrome (logic untouched) |
| `packages/frontend/src/components/SwapHistory.tsx` | Denser list chrome |
| `packages/frontend/src/components/HeaderWallet.tsx` / `WalletButton.tsx` | Quiet connect |
| `packages/frontend/src/components/DisclaimerBanner.tsx` | Quieter |
| `packages/frontend/src/components/RouteDisplay.tsx` / `TokenSelector.tsx` | Match surfaces |
| `packages/frontend/src/components/HoldingsSummary.tsx` | Compact link or home omit |

---

### Task 1: Design tokens

- [ ] Update `:root` + `body` background in `globals.css`
- [ ] Update `.btn-primary`, `.surface-panel*` to mint/charcoal
- [ ] Smoke: `npm run build` or `npx tsc` / next lint if existing
- [ ] Commit: `style(frontend): charcoal mint design tokens`

### Task 2: Shell + homepage IA

- [ ] Restyle `layout.tsx` nav
- [ ] Slim `page.tsx` to swap desk only
- [ ] Commit: `style(frontend): titan-like swap desk homepage`

### Task 3: SwapCard + satellites

- [ ] Restyle SwapCard / RouteDisplay / TokenSelector / Disclaimer / wallets / history
- [ ] Compact holdings treatment
- [ ] Commit: `style(frontend): restyle swap card and history chrome`

### Task 4: Verify

- [ ] Manual: home shows no Steps/Compare/FAQ; mint CTA; quote path intact
- [ ] `npm run build` in `packages/frontend` if feasible
- [ ] Confirm no new Limit UI

---

## Out of scope

Limit UI, deep portfolio/docs redesign, backend changes

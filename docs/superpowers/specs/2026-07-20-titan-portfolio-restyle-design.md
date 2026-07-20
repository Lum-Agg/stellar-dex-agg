# LumAgg Frontend: Titan-leaning Portfolio / Profile Restyle

**Date:** 2026-07-20  
**Status:** Approved  
**Depends on:** Homepage Titan restyle (`2026-07-20-titan-homepage-restyle-design.md`), Phase 2 portfolio data (`2026-07-19-phase2-portfolio-charts-design.md`)

## Problem

`/portfolio` still reads as a secondary dashboard page: small title block, emerald “Total value” pill, and a holdings table that doesn’t match the Titan Profile hierarchy (identity → large balance → tabbed content). Homepage swap desk was restyled; portfolio feels disconnected.

## Goal

Restyle `/portfolio` into a **Titan-leaning profile surface** with:

1. Wallet identity + prominent total balance hero  
2. Tabbed content: **Holdings** (active), **Swap history** (wired to existing API), **Limit orders** / **DCA** (disabled “Soon”)  
3. Holdings table: **Asset · Balance · Price · Value** with **24h sparkline embedded under the asset cell**  
4. Same charcoal + mint tokens as homepage; no new backend work

## Locked decisions (from brainstorming)

| Topic | Choice |
|-------|--------|
| Scope | **C** — structure + working tabs |
| Holdings columns | **C** — 4 columns; sparkline under asset name |
| Route | Keep `/portfolio` (nav label stays **Portfolio**) |
| Stats / VIP / Vanish / Burn | Out of scope (Titan-only product features) |
| Search bar | Out of scope for this pass |
| Nav rename to “Profile” | No — keep Portfolio to match URL and existing links |

## Approaches considered

### A — Page-only restyle (rejected)

Restyle `portfolio/page.tsx` in place; duplicate swap-history fetch logic inside the page for the history tab.

- **Pros:** Smallest diff  
- **Cons:** Two divergent history UIs (home Activity vs profile tab); harder to maintain

### B — Extract shared profile shell + tab panels (recommended)

Add small presentational components; refactor `SwapHistory` to support **compact** (home) and **table** (profile) variants sharing one data hook.

- **Pros:** Single source of truth for swap history; clean tab IA; matches Titan layout without cloning Titan features  
- **Cons:** Slightly more files than inline edit

### C — Full profile route rename + deep refactor (rejected)

Rename route to `/profile`, move all wallet-centric views under a layout with nested routes.

- **Pros:** Closest URL parity with Titan  
- **Cons:** Breaks existing links/docs; scope creep

**Recommendation:** **B**

## Page layout (connected state)

```
┌─────────────────────────────────────────────────────────────┐
│  [avatar]  GABC…XYZ                    (optional copy btn)  │
│            GABC…full address…XYZ (mono, muted, truncate)    │
├─────────────────────────────────────────────────────────────┤
│  $4.42                              (large, primary)        │
│  ≈ priced via LumAgg quotes         (muted subline)         │
├─────────────────────────────────────────────────────────────┤
│  Holdings | Swap history | Limit orders Soon | DCA Soon     │
├─────────────────────────────────────────────────────────────┤
│  [tab panel — table or list]                                │
└─────────────────────────────────────────────────────────────┘
```

### Hero / identity

- **Avatar:** deterministic circle from address (mint tint + initials or blockie-style hash color — no external deps; simple CSS gradient from address bytes is enough)
- **Short address:** first 4 + last 4, `text-[20–22px] font-semibold`
- **Full address:** mono, muted, single line truncate; copy-to-clipboard icon button
- **Total USD:** `text-[36–44px]` tabular-nums; show `—` while pricing loads; same aggregation logic as today
- **Subline:** e.g. “Valued via LumAgg quotes” — not chain-native balance in SOL terms (Stellar has no single native USD equivalent line like Titan’s SOL subline unless we add XLM notional — optional: show approximate XLM equivalent if XLM price known)

### Tabs

| Tab | State | Content |
|-----|-------|---------|
| Holdings | Active default | Holdings table (see below) |
| Swap history | Enabled | Full-height scrollable list/table of user swaps |
| Limit orders | Disabled | Label + “Soon” badge; no navigation |
| DCA | Disabled | Label + “Soon” badge; no navigation |

- Tab styling: underline active tab (mint accent), muted inactive; `text-[15–16px]`
- Tab state: client-side `useState` (no URL hash required for MVP)
- Persist tab in URL optional nice-to-have — **not in scope** unless trivial (`?tab=history`)

### Holdings table

| Column | Content |
|--------|---------|
| Asset | Token icon + symbol (primary), contract id truncated (muted mono), **Sparkline (80×28) below symbol row** |
| Balance | Formatted balance, tabular-nums |
| Price | USD mark from `/api/v1/prices` |
| Value | USD value, medium weight |

- Sort: by value descending (unchanged)
- Remove dedicated “24h” column header
- Row hover: subtle `bg-white/[0.02]`
- Loading: skeleton rows; empty: quiet empty state
- Pricing loading indicator: small “Updating prices…” near tab bar, not a loud banner

### Swap history tab

Refactor `SwapHistory`:

- Extract `useSwapHistory()` hook (fetch, refetch on `SWAP_SUCCESS_EVENT`, error states)
- **`variant="compact"`** — current home Activity card (unchanged behavior)
- **`variant="profile"`** — profile tab panel:
  - No outer “Activity” card chrome / max-height cap
  - Table or dense list: Time · Status · In → Out · link to stellar.expert
  - Full page width within profile container
  - Same empty / unavailable / refetch-error copy

Homepage continues using `<SwapHistory variant="compact" />`.

### Disconnected state

Keep connect CTA but restyle to match homepage empty surfaces (`surface-panel`, mint CTA). Copy: “Connect your wallet to view holdings and swap history.”

## Components (new / changed)

| File | Action |
|------|--------|
| `app/portfolio/page.tsx` | Restructure into hero + tabs; thin orchestration |
| `components/portfolio/ProfileHero.tsx` | **New** — avatar, address, total |
| `components/portfolio/ProfileTabs.tsx` | **New** — tab bar + Soon badges |
| `components/portfolio/HoldingsTable.tsx` | **New** — extract table from page; asset cell embeds Sparkline |
| `components/SwapHistory.tsx` | Refactor — hook + variants |
| `components/Sparkline.tsx` | Optional `size="sm"` prop for tighter asset cell |
| `components/HeaderNav.tsx` | No change (label stays Portfolio) |

## Visual tokens

Reuse homepage tokens from `globals.css`:

- Surfaces: `--bg-0`, `--surface`, `--surface-raised`, `--border`
- Text: `--text-primary`, `--text-secondary`, `--text-muted`
- Accent: `--accent` for active tab underline, success status, CTA
- **Remove** portfolio-specific emerald pill styling (`border-emerald`, `text-emerald-*`) in favor of neutral + mint accent

Typography scale (align with recent homepage bump):

- Page hero balance: 36–44px  
- Tab labels: 15–16px  
- Table body: 14–15px  
- Table headers: 12–13px uppercase tracking  

## Data flow (unchanged)

```
WalletProvider → address
AccountBalancesProvider → balances
portfolio page → fetchPrices + fetchPriceHistory (existing)
SwapHistory hook → fetchUserSwaps (existing)
```

No API or contract changes.

## Non-goals

- Limit order list UI (Phase 3d)
- DCA UI
- Titan Stats accordion, VIP upsell, Burn & Reclaim
- Token search / filter
- Pagination (holdings typically small; history cap stays at existing `limit=20` unless we raise later)
- `/stats` or `/docs` restyle
- Rename route to `/profile`

## Acceptance criteria

1. Connected user sees identity row + large total USD + four tabs on `/portfolio`
2. **Holdings** tab shows 4-column table with sparkline under each asset
3. **Swap history** tab lists the same swaps as home Activity (same API), without max-height clip
4. **Limit orders** and **DCA** tabs visible but disabled with “Soon”
5. Disconnected state shows connect CTA; no broken layout
6. Homepage `<SwapHistory variant="compact" />` still works
7. Visual consistency with charcoal + mint homepage (no emerald pill motif)
8. Mobile: tabs scroll horizontally if needed; table horizontally scrollable

## Risks

- `SwapHistory` refactor could regress home Activity — mitigate with variant prop + manual smoke test both surfaces
- Large hero type may wrap on small phones — test 320px width
- Sparkline in asset cell increases row height — acceptable tradeoff per user choice

## Test plan (manual)

1. Connect wallet → `/portfolio` → verify total matches sum of row values  
2. Switch Holdings ↔ Swap history tabs; history links open explorer  
3. Complete a swap on home → history tab refetches (event listener)  
4. Disconnect → connect CTA  
5. Home Activity still shows compact history  
6. Resize mobile — no horizontal page overflow  

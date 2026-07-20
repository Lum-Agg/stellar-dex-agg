# LumAgg Frontend: Titan-leaning Homepage Restyle

**Date:** 2026-07-20  
**Status:** Approved for planning  
**Direction:** Charcoal + Mint (Titan-leaning), in-place restyle, swap desk only

## Problem

The current site reads as a dark “dashboard landing page”: blue/purple radial glows, marketing sections (Steps / Compare / Venues / FAQ) under the swap card, and stacked secondary cards. That density + glow styling produces a lightweight / “toy” feel compared with mature aggregators (especially Titan’s quiet swap desk).

## Goal

Restyle the **shell + homepage swap experience** so the first viewport feels like a professional trading surface: calm charcoal UI, mint CTA, swap-first IA. Preserve existing quote/sign/history logic; this is a visual + IA pass, not a backend rewrite.

## Decisions

| Topic | Choice |
|-------|--------|
| Reference | Titan-leaning (not Jupiter density) |
| Palette | Charcoal + mint |
| Scope | Homepage + global shell tokens / nav |
| IA | Swap desk only (no marketing blocks on home) |
| Implementation | In-place restyle (same components, new chrome) |
| Limit UI | Out of scope (later phase) |

## Visual system

### Color tokens (replace `:root` in `globals.css`)

| Token | Role | Target |
|-------|------|--------|
| `--bg-0` | Page | `#0a0b0d` |
| `--bg-1` / surface | Panel | `#12151a` |
| `--surface-raised` | Nested | `#1a1f27` |
| `--border` | Hairline | low-contrast cool gray |
| `--text-primary` | Titles / amounts | near-white |
| `--text-secondary` / muted | Labels | zinc mid |
| `--accent` | Primary CTA | mint ≈ `#3dd6c6` |
| `--accent-contrast` | Text on CTA | near-black |

### Explicit removals

- Purple / blue **radial glow** backgrounds on `body`
- Overly bright blue primary buttons (`#3b82f6` as brand accent)
- Decorative “glow” shadows as the main visual identity

### Motion

Keep restrained: 1–2 subtle transitions (hover on connect/CTA, quote refresh opacity). No particle/glow marketing motion.

## Information architecture (homepage)

**Keep**

- Top shell: brand, primary nav links that already exist (Trade / Portfolio / Docs as applicable), wallet connect
- Centered `SwapCard`
- Slim `SwapHistory` under the card (restyled; not competing with the swap module)

**Remove from homepage** (this pass)

- “Three steps” section
- `CompareSection`
- Liquidity sources / venues chips section
- `FaqSection` (content may remain reachable via Docs if already covered; no need to port FAQ onto home)

**Holdings**

- Prefer a single quiet link/row to `/portfolio` instead of a full `HoldingsSummary` card on home (avoids stacked toy cards). If removing summary breaks a Phase 2 acceptance path, keep a one-line compact strip — not a second panel stack.

**Disclaimer**

- Keep compliance disclaimer, but quieter (smaller type / less banner chrome).

## Component restyle notes

| Surface | Change |
|---------|--------|
| `layout` / nav | Flat, hairline bottom border; mint unused in nav chrome |
| `SwapCard` | Larger soft radius, Sell/Buy labels, mint CTA, reduce nested borders |
| `RouteDisplay` | Collapsed/quiet by default; no loud route circus |
| `TokenSelector` | Match new surfaces; avoid thick glowing modals |
| `SwapHistory` | List-like density; same API data |
| `HeaderWallet` / `WalletButton` | Border button, not candy pill |

## Non-goals

- Rebuild of quote engine, wallet signing, API contracts
- New Limit / DCA UI
- Full restyle of `/portfolio`, `/docs`, `/stats` (secondary pass; they should inherit CSS tokens so they don’t clash badly)
- Pixel-perfect Titan clone / trademarked assets
- Light mode

## Acceptance

1. Homepage first viewport = brand + nav + swap module + optional slim history; no Steps/Compare/Venues/FAQ  
2. No purple/blue page glow; accent is mint on primary actions  
3. Swap quote → sign path still works unchanged  
4. Portfolio/docs still load (may look partially unstyled vs home, but not broken)  
5. Mobile: swap card usable, no horizontal overflow  

## Risks

- Token inheritance may leave portfolio pages half-restyled — acceptable for this phase  
- History under card can still clutter if not densified — keep height capped / collapse when empty  

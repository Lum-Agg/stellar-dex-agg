# Swap Settings Modal (Jupiter-style)

**Date:** 2026-07-23  
**Status:** Approved  
**Scope:** Frontend Instant swap card only

## Goal

Expose **max hops** and **max splits** to retail users, and move full slippage controls into a settings modal, while keeping current slippage visible on the swap card (option A / Jupiter pattern).

## Locked decisions

| Topic | Choice |
|-------|--------|
| Card chrome | Single chip: `{slippage}%` + settings icon → opens modal |
| Slippage in modal | Presets `0.5%` / `1%` + Custom (not Soroswap Auto/Custom) |
| Routing | `Max hops` (1–4, default 3), `Max splits` (1–5, default 3) |
| Persistence | `localStorage` key `lumagg.swapSettings` |
| API | Pass `slippage`, `max_hops`, `max_splits` on `GET /api/v1/quote` (already supported) |
| Out of scope | Ultra/Manual, priority fees, AMM source toggles, card footer “Route: Auto” line (v1) |

## UX

```
[ Swap ]                    [ 1% ⚙ ]
  Sell / Buy …
  Review & swap
  Route details …
```

Modal `Swap Settings`:
1. Max slippage — presets + custom %
2. Max hops — number input
3. Max splits — number input

Changing any value closes-or-keeps modal open but invalidates quote and re-fetches with new params.

## Defaults & clamps

| Field | Default | Min | Max |
|-------|---------|-----|-----|
| slippage % | 1.0 | 0.01 | 50 |
| max_hops | 3 | 1 | 4 |
| max_splits | 3 | 1 | 5 |

Presets shown: `0.5`, `1.0` (match Jupiter; keep ability to type `0.1` via Custom).

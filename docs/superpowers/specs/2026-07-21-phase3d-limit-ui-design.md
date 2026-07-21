# LumAgg Phase 3d: Limit Orders UI (Testnet)

**Date:** 2026-07-21  
**Status:** Approved for planning  
**Depends on:** Phase 3c orders API + SDK, testnet escrow deploy (`docs/limit-orders-testnet.md`)

## Problem

Order rail shows Limit as “Soon” while create/list/cancel APIs and escrow already exist on **testnet**. Retail cannot exercise the custody + sign path from the UI.

## Goal

Ship a **testnet-only** Limit panel on the homepage: create limit order, list open orders, cancel — wallet signs unsigned XDR (same pattern as Instant swap).

## Locked decisions

| Topic | Choice |
|-------|--------|
| Network | **Testnet only** (no mainnet escrow) |
| Placement | Same page (`/`); Order rail Instant ↔ Limit |
| MVP depth | Create + list open + cancel on homepage |
| Approach | Separate `LimitCard` (+ open-orders list), not inflate `SwapCard` |
| DCA | Remains Soon |
| Portfolio Limit tab | Remains Soon this pass |

## Approaches considered

### A — LimitCard + rail state (recommended)

`page.tsx` holds `orderType: 'instant' | 'limit'`. Limit uses dedicated components; Instant unchanged.

### B — Mode flag inside SwapCard (rejected)

Couples Instant quote/sign with Limit escrow flow.

### C — UI mock only (rejected)

Does not close the product loop.

## UX / IA

```
[ Order rail ]     [ Limit panel ]
 Instant           Testnet banner
 Limit (active)    Sell amount + token
 DCA (Soon)        Buy token
                   Limit price (human: 1 IN = x OUT)
                   Expiry presets (1h / 1d / 7d)
                   [ Place limit ]
                   ─────────────
                   Open orders list + Cancel
```

- Switching rail only swaps the center column; layout shell unchanged.
- Limit mode shows a clear **Testnet** notice (network passphrase / Freighter must be Testnet).
- Below the form: open orders for the connected wallet (not Instant Activity).

## Create form → API

| UI field | API field |
|----------|-----------|
| Sell token | `token_in` |
| Buy token | `token_out` |
| Sell amount | `amount_in` (stroops string) |
| Limit price (OUT per 1 IN, human) | `limit_out_per_in_e7` |
| Expiry preset | `expires_ledger` = current_ledger + Δ |

**Price conversion:**  
`limit_out_per_in_e7 = round(price_human * 10^7)` where `price_human` is units of `token_out` per 1 whole `token_in` (adjust if decimals differ — both SAC default 7 on Stellar; use token decimals from token list).

**Expiry:** Fetch latest ledger (Horizon testnet or helper on limit API). Presets:

| Preset | Approx Δ ledgers (5s/ledger) |
|--------|------------------------------|
| 1h | ~720 |
| 1d | ~17_280 |
| 7d | ~120_960 |

Flow: `POST /orders/build_create` → `signTx` → submit via testnet RPC/Horizon → refresh open list after short delay.

## Open orders + cancel

- `GET /orders?user=G…&status=open`
- Row: pair, remaining in, limit price (decode e7 → human), expires ledger, **Cancel**
- Cancel: `POST /orders/build_cancel` → sign → submit → refresh

Empty / disconnected / API missing: quiet empty states (match Instant tone).

## Config (frontend)

| Env | Role |
|-----|------|
| `NEXT_PUBLIC_API_URL` | Instant / mainnet (unchanged) |
| `NEXT_PUBLIC_LIMIT_API_URL` | Testnet api-server base (orders + optional health) |
| `NEXT_PUBLIC_LIMIT_HORIZON_URL` | Optional; default `https://horizon-testnet.stellar.org` for ledger height |
| `NEXT_PUBLIC_LIMIT_RPC_URL` | Optional; default testnet Soroban RPC for submit |
| `NEXT_PUBLIC_LIMIT_NETWORK` | `testnet` (documentation / banner) |

If `NEXT_PUBLIC_LIMIT_API_URL` unset: Limit rail enabled but panel shows “Testnet Limit API not configured”.

Wallet: reuse `WalletProvider`; Limit submit must use **testnet** network passphrase when signing (`Networks.TESTNET` / kit equivalent). **Risk:** shared wallet context today is PUBLIC — Phase 3d must either:

1. Temporarily set kit network to TESTNET while in Limit mode, or  
2. Pass `networkPassphrase` on `signTransaction` for Limit txs only.

**Decision:** Prefer (2) if kit supports per-sign passphrase; else (1) with restore to PUBLIC when leaving Limit mode. Document Freighter Testnet requirement in banner.

## Components / files

| File | Action |
|------|--------|
| `app/page.tsx` | `orderType` state; conditional Instant vs Limit |
| `components/OrderTypeRail.tsx` | Enable Limit; `onSelect` callback |
| `components/LimitCard.tsx` | **New** — create form + place |
| `components/OpenOrders.tsx` | **New** — list + cancel |
| `lib/limit-orders.ts` | **New** — fetch helpers (or thin wrappers around SDK types) |
| `lib/wallet-context.tsx` | Support testnet passphrase for Limit sign |
| Portfolio / DCA | No change |

Reuse: `TokenSelector`, connect CTA patterns, surface tokens from Titan restyle.

## Non-goals

- Mainnet limit escrow / production `ESCROW_CONTRACT` for limits  
- DCA UI  
- Portfolio “Limit orders” tab data  
- Keeper fill UI / notifications  
- Changing production Instant quote path  

## Acceptance

1. On `/`, Limit is selectable; Instant still works against mainnet API.  
2. With testnet API configured + Freighter Testnet: create → open order appears → cancel removes/updates.  
3. Without Limit API env: clear unavailable message, no mainnet escrow calls.  
4. DCA remains Soon.  
5. Mobile: rail horizontal + Limit form usable.

## Risks

- Wallet network mismatch (mainnet Freighter vs testnet XDR) — banner + passphrase on sign.  
- Testnet liquidity thin — fill not required for acceptance (create/cancel/list is enough).  
- `expires_ledger` clock skew — use Horizon latest + buffer.

## Test plan (manual)

1. Configure local/staging frontend with testnet Limit env; main Instant still hits prod or local mainnet API as today.  
2. Friendbot-funded testnet account; place 1h limit; see list.  
3. Cancel; list updates.  
4. Switch back to Instant; place a mainnet quote path still healthy.  
5. Unset Limit API URL; confirm guardrail message.

# Integrator DX: OpenAPI + SDK 0.2 + Freighter browser demo

**Date:** 2026-07-22  
**Status:** Approved for planning  
**Goal:** Make third-party integration copy-paste ready (docs + SDK + runnable Freighter path). External pilots and arb volume work are out of scope for this spec.

## Context

Public API at `https://api.lumagg.xyz` already exposes wallet helpers (`balance`, `submit_tx`, `tx_status`, etc.), but:

- `docs/openapi.yaml` omits those paths
- `@lumagg/sdk` `0.1.0` stops at quote / build / stats / orders / prices
- Examples stop at unsigned XDR; no minimal Freighter → submit → confirm sample

External app density on Stellar is low; improving DX is the near-term growth lever (vs cold-outreach pilots).

## Decisions

| Topic | Choice |
|-------|--------|
| Scope | OpenAPI sync + SDK methods + Freighter browser demo + guide / site docs |
| SDK version | `0.1.0` → **`0.2.0`** (additive) and npm publish |
| Signing | **Not** in SDK — app uses `@stellar/freighter-api` |
| Freighter demo | `packages/sdk/examples/browser-swap/` (Vite SPA), not a frontend route |
| Official RPC dual-path | SDK uses LumAgg `submit_tx` / `tx_status` only (frontend Advanced toggle stays frontend-only) |
| Arb / pilots / embed / referral | Out of scope |

## 1. OpenAPI

Update `docs/openapi.yaml` to document existing routes (no server behavior changes):

| Method | Path | Notes |
|--------|------|-------|
| GET | `/api/v1/balance` | `account`, `token` → `balance`, optional `has_trustline` |
| GET | `/api/v1/balances` | `account` → map of balances + trustlines |
| GET | `/api/v1/account` | `account` → `sequence` |
| GET | `/api/v1/classic_asset` | `contract` → `code`, `issuer` |
| GET | `/api/v1/ledger/latest` | latest closed ledger |
| POST | `/api/v1/submit_tx` | `{ signed_tx_xdr }` → fast enqueue `{ hash, status? }` |
| GET | `/api/v1/tx_status` | `hash` → `{ status, confirmed, error? }` |

Add tags `wallet` and `tx`. Schemas must match current JSON field names from `crates/api-server/src/handlers.rs`.

Bump OpenAPI `info.version` to `1.1.0`.

## 2. SDK (`@lumagg/sdk`)

Extend `LumAggClient` in `packages/sdk/src/index.ts`:

| Method | Behavior |
|--------|----------|
| `getBalance({ account, token })` | GET `/balance` |
| `getBalances({ account })` | GET `/balances` |
| `getAccount({ account })` | GET `/account` |
| `getClassicAsset({ contract })` | GET `/classic_asset` |
| `getLatestLedger()` | GET `/ledger/latest` |
| `submitTx({ signedTxXdr })` | POST `/submit_tx` |
| `getTxStatus({ hash })` | GET `/tx_status` |
| `waitForTx(hash, opts?)` | Poll `getTxStatus` until `confirmed` / `FAILED` / timeout (default 60s, interval 1s) |

Conventions (match existing client):

- camelCase params / return fields; map snake_case wire format internally
- On `success === false` or non-OK HTTP: `throw new Error(...)` using body `error` when present
- Optional `apiKey` header unchanged

Publish: bump `package.json` to `0.2.0`, `npm run build`, `./scripts/publish-sdk.sh --publish` (requires npm auth).

## 3. Freighter browser demo

**Path:** `packages/sdk/examples/browser-swap/`

**Stack:** Vite + TypeScript; depends on workspace `@lumagg/sdk` and `@stellar/freighter-api`.

**UI (minimal):** connect button, amount input, log panel, “Swap XLM→USDC” action. No cards / marketing chrome beyond what’s needed to run the flow.

**Flow:**

1. Freighter `requestAccess` → `G…` address  
2. `getBalance` for XLM; for USDC check `hasTrustline` and surface a clear message if missing  
3. `quoteAndBuild` with `preferSoroban: true` (amount configurable; default small)  
4. `signTransaction(unsignedTxXdr, { network: 'PUBLIC' })`  
5. `submitTx` → `waitForTx(hash)`  
6. Print hash + final status  

**Run:** `cd packages/sdk/examples/browser-swap && npm i && npm run dev`

**Guardrails:**

- Default can stop before submit via a “dry-run / stop after sign” checkbox so reviewers without funding can still exercise quote→build→sign  
- Mainnet only for this demo (matches prod API)

## 4. Docs

| File | Change |
|------|--------|
| `docs/integrator-guide.md` | Browser (Freighter) section; SDK method table for new APIs |
| `docs/integrator-guide.zh-CN.md` | Mirror |
| `packages/sdk/README.md` | 0.2.0 methods + browser-swap link |
| `packages/frontend/.../ApiReference.tsx` | Add short entries for the new endpoints (hand-maintained page; not a full redesign) |

Existing `scripts/integrator-smoke.sh` and `examples/quote-build.ts` remain unsigned-XDR smoke tests.

## 5. Verification

| Check | Pass criteria |
|-------|----------------|
| SDK build | `cd packages/sdk && npm run build` |
| OpenAPI | Paths/params match handlers (manual diff) |
| CLI smoke | Existing quote-build still works against prod |
| Browser demo | Connect + quoteAndBuild succeeds with Freighter; optional live submit |
| Publish | `npm view @lumagg/sdk version` shows `0.2.0` after publish |

## 6. Out of scope

- External integrator pilots / outreach scripts  
- Arb bot optimization for volume  
- Embed widget, referral, partner self-serve API keys  
- SDK dual-submit via official Soroban RPC  
- Multi-wallet (Lobstr / xBull) in the demo  
- Shipping browser-swap as a published npm package  

## 7. Success

An integrator can: read OpenAPI or `/docs/api`, `npm i @lumagg/sdk@0.2.0`, and copy the Freighter demo flow to complete quote → build → sign → submit → confirm on mainnet without reading api-server source.

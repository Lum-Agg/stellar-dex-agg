# LumAgg integrator guide

Quickstart for wallets, dApps, and trading bots integrating the public REST API.

**Live API:** https://api.lumagg.xyz  
**OpenAPI:** [openapi.yaml](./openapi.yaml) · **Web docs:** https://lumagg.xyz/docs  
**Benchmark pack:** [scf-benchmark-results.md](./scf-benchmark-results.md) · [scf-venue-comparison.md](./scf-venue-comparison.md)

## 1. Quote → build → sign

```bash
API=https://api.lumagg.xyz
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

# 1) Quote (1 XLM → USDC)
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "slippage=0.5"

# 2) Soroban-only quote (exclude Classic SDEX — fair vs Soroswap API)
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "prefer_soroban=1"

# 3) Build unsigned XDR (POST body from quote sub_routes — see OpenAPI)
```

Flow: **`GET /quote`** → map `sub_routes` to **`POST /build_tx`** → wallet signs XDR → submit via **`POST /api/v1/submit_tx`** (LumAgg proxies your Soroban RPC) or any same-network Soroban RPC.

### One-command smoke test (recommended for external integrators)

```bash
chmod +x scripts/integrator-smoke.sh
USER_G=GYourFundedAddress ./scripts/integrator-smoke.sh

# Save JSON evidence for grant (D2):
OUT=./evidence/pilot-b USER_G=G... ./scripts/integrator-smoke.sh
```

`USER_G` must be a mainnet account with a sequence number (any small XLM balance is enough). Success prints `unsigned_tx_xdr` prefix.

For swaps into classic-backed SACs (USDC/EURC), the account must already have a **trustline** for the buy asset — otherwise simulate fails with a clear error. Add trustline in Freighter first (~0.5 XLM reserve). Check trustline status via `has_trustline` on `/api/v1/balance` and `/api/v1/balances` (derived from the same SAC `balance` simulate; no extra Horizon call).

SDK alternative:

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
```

## 2. `prefer_soroban`

| Value | Behavior |
|-------|----------|
| omitted or `0` | Best price across **Soroban AMMs + Classic SDEX** |
| `1` | **Soroban only** — no PathPayment / SDEX paths |

Use `prefer_soroban=1` when comparing against Soroban-only aggregators, or when your wallet cannot sign Classic PathPayment in the same flow.

Soroswap API uses `protocols: ["soroswap","phoenix","aqua"]` (omit `"sdex"`) for the same effect. See [Soroswap API docs](https://docs.soroswap.finance/soroswap-api).

## 3. Rate limits & API keys

| Tier | Limit | Auth |
|------|-------|------|
| Anonymous | 10 req/s per IP | none |
| Partner | 60 req/s per key | `X-API-Key: <key>` header |

HTTP `429` when exceeded. Invalid `X-API-Key` returns `401` when partner keys are configured on the server.

**Partner key issuance:** contact the LumAgg team (GitHub issue or grant correspondence). Keys are deployed server-side via:

```bash
LUMAGG_PARTNER_API_KEYS=key_one,key_two
```

## 4. Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/v1/health` | Liveness |
| GET | `/api/v1/tokens` | Routable tokens + **self-hosted** logo URLs |
| GET | `/logos/{file}` | Static token logo files (`image/png|jpeg|webp|svg+xml`) |
| GET | `/api/v1/quote` | Best route |
| POST | `/api/v1/build_tx` | Unsigned XDR |
| GET | `/api/v1/balance` | Single SAC balance (`has_trustline` when known) |
| GET | `/api/v1/balances` | Batch balances + per-token `has_trustline` map |
| GET | `/api/v1/account` | Account sequence (via Soroban RPC `getLedgerEntries`) |
| GET | `/api/v1/ledger/latest` | Latest closed ledger sequence |
| POST | `/api/v1/submit_tx` | Submit signed XDR (`{ "signed_tx_xdr": "..." }`) via server RPC |
| GET | `/api/v1/prices` | Latest USDC marks (batch) |
| GET | `/api/v1/prices/history` | Sampled price ticks for charts |
| GET | `/api/v1/orders` | Limit orders for a wallet (indexer DB) |
| POST | `/api/v1/orders/build_create` | Unsigned XDR for `create_limit` |
| POST | `/api/v1/orders/build_cancel` | Unsigned XDR for `cancel` |

`/api/v1/tokens[].logo` is either empty during early enrichment, or an absolute self-hosted URL under:

```text
https://api.lumagg.xyz/logos/
```

Optional `logo_kind`:
- `"official"` — downloaded from SEP-42 lists (Soroswap / LOBSTR / StellarExpert Top50) and self-hosted as-is (PNG/JPEG/WebP/GIF/SVG)
- `"fallback"` — locally generated letter avatar when no curated icon is available

Do not rely on third-party image hosts for token icons.

## 5. Execution modes

- **Soroban:** `build_tx` returns `execution: "soroban"` — single `aggregator.swap` invoke (multi-leg / split).
- **Classic:** `execution: "classic"` — `PathPaymentStrictSend` when quote used SDEX only.
- **No hybrid:** Classic + Soroban cannot be combined in one Stellar transaction.

## 6. Differentiation evidence

Reproduce quote benchmarks locally:

```bash
./scripts/scf-benchmark.sh
LUMAGG_PREFER_SOROBAN=1 SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
```

See [scf-venue-comparison.md](./scf-venue-comparison.md) for venue matrix vs Stellar Broker and split-routing notes.

## 7. npm SDK (Tranche 2)

Published: [`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk) `0.1.0` (`packages/sdk`).

```bash
npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/basic-usage.ts
```

See [packages/sdk/README.md](../packages/sdk/README.md).

## 8. On-chain stats

Public rollup when API has indexer DB mounted:

```bash
curl -s https://api.lumagg.xyz/api/v1/stats | jq .
```

Sample export: [sample-indexer-export.json](./sample-indexer-export.json) · pipeline: [analytics-indexer.md](./analytics-indexer.md).

### Wallet swap history

Recent aggregator invocations for a connected wallet (same indexer DB as `/stats`):

```bash
curl -s "https://api.lumagg.xyz/api/v1/swaps?user=G...&limit=20" | jq .
```

Returns `data.swaps[]` with `tx_hash`, token amounts, `status`, and `is_split`. Empty history is `200` with `"swaps": []`. Requires `INDEXER_DB_PATH` on the server (otherwise `503`).

### Limit orders

List open limit orders and build unsigned create/cancel XDR for the order-escrow contract:

```bash
curl -s "https://api.lumagg.xyz/api/v1/orders?user=G...&status=open" | jq .

curl -sX POST "https://api.lumagg.xyz/api/v1/orders/build_create" \
  -H 'Content-Type: application/json' \
  -d '{
    "user": "G...",
    "token_in": "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
    "token_out": "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
    "amount_in": "10000000",
    "limit_out_per_in_e7": "20000000",
    "expires_ledger": 12345678
  }' | jq .

curl -sX POST "https://api.lumagg.xyz/api/v1/orders/build_cancel" \
  -H 'Content-Type: application/json' \
  -d '{"user": "G...", "order_id": 1}' | jq .
```

`GET /orders` reads from the same indexer SQLite as `/swaps` (`INDEXER_DB_PATH`). Build endpoints require `ESCROW_CONTRACT` on the server. Response shape matches `build_tx`: `unsigned_tx_xdr`, `fee`, `execution`, `num_operations`, `contract`. SDK: `listOrders`, `buildCreateOrder`, `buildCancelOrder`.

**Orders env (api-server operator):**

| Variable | Purpose |
|----------|---------|
| `INDEXER_DB_PATH` | SQLite with `limit_orders` table (required for list) |
| `ESCROW_CONTRACT` | Deployed order-escrow contract id (required for build endpoints) |

### Token prices & chart history

Quote-engine USDC marks for portfolio valuation and simple sparklines:

```bash
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

# Latest marks (batch, max 50 ids)
curl -sG "https://api.lumagg.xyz/api/v1/prices" \
  --data-urlencode "ids=$XLM,$USDC" | jq .

# Sampled history for a sparkline (default range=24h)
curl -sG "https://api.lumagg.xyz/api/v1/prices/history" \
  --data-urlencode "id=$XLM" \
  --data-urlencode "range=7d" | jq .
```

`GET /prices` returns `data.prices[]` with `id`, `price_usdc`, `ts`, and `via` (`usdc` or `xlm`). Missing ticks trigger a one-shot on-demand quote. Unpriceable tokens are omitted.

`GET /prices/history` returns `data.points[]` with `ts` and `price_usdc`. Empty history is `200` with `"points": []`. Range must be `24h` or `7d`.

**Sampler env (api-server operator):**

| Variable | Purpose |
|----------|---------|
| `PRICE_DB_PATH` | SQLite path for sampled ticks (required for history + background sampler) |
| `PRICE_SAMPLER` | Set to `0` to disable background sampling (default: enabled when `PRICE_DB_PATH` is set) |
| `PRICE_SAMPLE_SECS` | Sample interval in seconds (default `600`) |
| `PRICE_SAMPLE_TOKEN_LIMIT` | Max extra registry tokens to sample beyond priority list (default `30`) |
| `PRICE_RETENTION_DAYS` | Optional positive integer to prune ticks older than N days (default: keep forever) |

## 9. Atomic arb operators

Self-deploy vault + bot: [arb-operator.md](./arb-operator.md).

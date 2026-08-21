# LumAgg integrator guide

Quickstart for wallets, dApps, and trading bots integrating the public REST API.

**Live API:** https://api.lumagg.xyz  
**OpenAPI:** [openapi.yaml](./openapi.yaml) · **Docs:** https://lumagg.gitbook.io/  
**API reference:** [api-reference.md](./api-reference.md)

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

# 2) Optional Soroban-only quote (mainly for arbitrage / Soroban-only integrations)
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "prefer_soroban=1"

# 3) Build unsigned XDR — map quote.sub_routes into POST /api/v1/build_tx
curl -sX POST "$API/api/v1/build_tx" \
  -H 'Content-Type: application/json' \
  -d '{
    "user_public_key": "GYourFundedAddress",
    "token_in": "'"$XLM"'",
    "token_out": "'"$USDC"'",
    "amount_in": "10000000",
    "slippage": 0.5,
    "sub_routes": []
  }'
```

Replace `sub_routes` with the array returned by `/quote`. Full schemas: [OpenAPI](./openapi.yaml).

Flow: **`GET /quote`** → map `sub_routes` to **`POST /build_tx`** → wallet signs XDR → submit directly to any same-network Soroban RPC. **`POST /api/v1/submit_tx`** remains an optional LumAgg proxy when the integrator cannot reach an RPC directly.

### One-command smoke test

```bash
chmod +x scripts/integrator-smoke.sh
USER_G=GYourFundedAddress ./scripts/integrator-smoke.sh

# Optional: save JSON output for your records
OUT=./tmp/smoke USER_G=G... ./scripts/integrator-smoke.sh
```

`USER_G` must be a mainnet account with a sequence number (any small XLM balance is enough). Success prints `unsigned_tx_xdr` prefix.

For swaps into classic-backed SACs (USDC/EURC), the account must already have a **trustline** for the buy asset — otherwise simulate fails with a clear error. Add trustline in Freighter first (~0.5 XLM reserve). Check trustline status via `has_trustline` on `/api/v1/balance` and `/api/v1/balances` (derived from the same SAC `balance` simulate; no extra Horizon call).

SDK alternative:

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
```

### Browser (Freighter) — full sign + submit

CLI smoke stops at unsigned XDR. For the full loop (quote → build → Freighter sign → direct Soroban RPC submit → confirmation):

```bash
cd packages/sdk && npm run build
cd examples/browser-swap && npm install && npm run dev
```

Open the Vite URL, connect Freighter (Public), leave **Dry-run** checked to stop after sign, or uncheck to submit directly on mainnet. See [`packages/sdk/examples/browser-swap/README.md`](../packages/sdk/examples/browser-swap/README.md). The example keeps `/api/v1/submit_tx` as an optional fallback.

## 2. `prefer_soroban`

| Value | Behavior |
|-------|----------|
| omitted or `0` | Best price across **Soroban AMMs + Classic SDEX** |
| `1` | **Soroban only** — no PathPayment / SDEX paths |

`prefer_soroban` defaults to `false`. Ordinary frontend integrations should
usually omit it and use the best route across supported venues. Set
`prefer_soroban=1` mainly for the LumAgg arbitrage bot or when the integration
explicitly requires Soroban-only routes.

The route search can also be bounded with `max_hops` and `max_splits`:

```bash
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "max_hops=3" \
  --data-urlencode "max_splits=2"
```

Both parameters are optional. Lower values can reduce quote latency and route
complexity, while higher values may discover better execution at the cost of
more search work. The SDK exposes them as `maxHops` and `maxSplits`.

Soroswap API uses `protocols: ["soroswap","phoenix","aqua"]` (omit `"sdex"`) for the same effect. See [Soroswap API docs](https://docs.soroswap.finance/soroswap-api).

## 3. Rate limits & API keys

| Tier | Limit | Auth |
|------|-------|------|
| Anonymous | 10 req/s per IP | none |
| Partner | 60 req/s per key | `X-API-Key: <key>` header |

HTTP `429` when exceeded. Invalid `X-API-Key` returns `401` when partner keys are configured on the server.

**Partner key issuance:** open a [GitHub issue](https://github.com/Lum-Agg/stellar-dex-agg/issues) or contact the LumAgg team. Keys are deployed server-side via:

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
| GET | `/api/v1/classic_asset` | Resolve SAC `C…` → classic `code` / `issuer` |
| GET | `/api/v1/ledger/latest` | Latest closed ledger sequence |
| POST | `/api/v1/submit_tx` | Submit signed XDR (`{ "signed_tx_xdr": "..." }`) — fast enqueue |
| GET | `/api/v1/tx_status` | Poll inclusion after `submit_tx` (`confirmed` when SUCCESS) |
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

## 6. Reproducing quote benchmarks

Use these scripts when you want to compare routing quality (for example Soroban-only vs multi-venue quotes), not as part of day-to-day integration.

```bash
./scripts/scf-benchmark.sh
LUMAGG_PREFER_SOROBAN=1 SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
```

Venue coverage notes: [Performance / venue comparison](./scf-venue-comparison.md). For production integrations, `prefer_soroban=1` on `/quote` is usually enough when you need a Soroban-only scope.

## 7. npm SDK

Published: [`@lumagg/sdk`](https://www.npmjs.com/package/@lumagg/sdk) `0.2.0` (`packages/sdk`).

| SDK method | REST |
|------------|------|
| `quote` / `buildTx` / `quoteAndBuild` | `/quote`, `/build_tx` |
| `getBalance` / `getBalances` | `/balance`, `/balances` |
| `getAccount` / `getClassicAsset` / `getLatestLedger` | `/account`, `/classic_asset`, `/ledger/latest` |
| `submitTx` / `getTxStatus` / `waitForTx` | `/submit_tx`, `/tx_status` |
| `listTokens` / `getStats` / `listSwaps` / orders / prices | see OpenAPI |

```bash
USER_G=G... npx tsx packages/sdk/examples/quote-build.ts
npx tsx packages/sdk/examples/basic-usage.ts
# Freighter end-to-end:
cd packages/sdk/examples/browser-swap && npm run dev
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

Returns `data.swaps[]` with `tx_hash`, token amounts, `status`, and `is_split`. Empty history is `200` with `"swaps": []`. Requires the `[indexer]` TOML section on the server (otherwise `503`).

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

`GET /orders` reads from the same indexer SQLite as `/swaps` (`indexer.db_path`). Build endpoints require `features.escrow_contract`. Response shape matches `build_tx`: `unsigned_tx_xdr`, `fee`, `execution`, `num_operations`, `contract`. SDK: `listOrders`, `buildCreateOrder`, `buildCancelOrder`.

**Orders env (api-server operator):**

| Variable | Purpose |
|----------|---------|
| `indexer.db_path` | SQLite with `limit_orders` table (required for list) |
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

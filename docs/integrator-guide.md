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

Flow: **`GET /quote`** → map `sub_routes` to **`POST /build_tx`** → wallet signs XDR → submit via Soroban RPC or Horizon.

### One-command smoke test (recommended for external integrators)

```bash
chmod +x scripts/integrator-smoke.sh
USER_G=GYourFundedAddress ./scripts/integrator-smoke.sh

# Save JSON evidence for grant (D2):
OUT=./evidence/pilot-b USER_G=G... ./scripts/integrator-smoke.sh
```

`USER_G` must be a mainnet account with a sequence number (any small XLM balance is enough). Success prints `unsigned_tx_xdr` prefix.

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
| GET | `/api/v1/balance` | Single SAC balance |
| GET | `/api/v1/balances` | Batch balances for common tokens |

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

Published package: `packages/sdk` → `@lumagg/sdk` (Aug 2026).

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

## 9. Atomic arb operators

Self-deploy vault + bot: [arb-operator.md](./arb-operator.md).

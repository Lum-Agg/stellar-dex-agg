# API Reference

The LumAgg REST API is described by the public
[OpenAPI 3 specification](openapi.yaml). Import that file into Swagger UI,
Postman, Insomnia, or an OpenAPI client generator for complete schemas and
examples.

Production base URL:

```text
https://api.lumagg.xyz/api/v1
```

Self-hosted deployments use the address configured by `LISTEN_ADDR`.

## Quickstart

The browser and bot integration flow is:

```text
/api/v1/tokens -> /api/v1/quote -> /api/v1/build_tx -> wallet sign -> Stellar RPC submit
```

Example quote for 1 XLM to USDC:

```bash
API=https://api.lumagg.xyz
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "slippage=0.5"
```

Use the returned `sub_routes` with `POST /api/v1/build_tx`. The response
contains an unsigned transaction XDR. The application wallet signs it; LumAgg
does not receive or handle secret keys.

## Core Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/v1/quote` | Find and quote a single, multi-hop, or split route |
| `POST` | `/api/v1/build_tx` | Build an unsigned transaction XDR for a quote |
| `POST` | `/api/v1/submit_tx` | Optional signed-XDR proxy to the configured Soroban RPC |
| `GET` | `/api/v1/tx_status?hash=...` | Poll a submitted transaction |
| `GET` | `/api/v1/tokens` | Return routable token metadata |
| `GET` | `/api/v1/balances?account=G...` | Return account balances used by the UI |
| `GET` | `/api/v1/health` | Process liveness |
| `GET` | `/api/v1/ready` | Routing-data readiness |
| `GET` | `/api/v1/orders?user=G...` | Indexed Limit orders for a wallet |
| `POST` | `/api/v1/orders/build_create` | Build unsigned Limit order creation XDR |
| `POST` | `/api/v1/orders/build_cancel` | Build unsigned Limit order cancellation XDR |
| `GET` | `/api/v1/dca?user=G...` | Indexed DCA orders for a wallet |
| `POST` | `/api/v1/dca/build_create` | Build unsigned DCA creation XDR |
| `POST` | `/api/v1/dca/build_cancel` | Build unsigned DCA cancellation XDR |

The normal integration flow is:

```text
/tokens -> /quote -> /build_tx -> wallet sign -> submit to Stellar
```

LumAgg never needs the user's secret key. `/build_tx` returns unsigned XDR;
the user's wallet signs and submits it. Direct submission to the wallet provider's
Stellar RPC is preferred; `/submit_tx` is an optional convenience proxy. See the [Integrator Guide](integrator-guide.md)
for request examples, amount units, slippage, maximum hops, maximum splits,
errors, and partner API keys.

## Rate Limits

| Tier | Limit | Authentication |
| --- | --- | --- |
| Anonymous | 10 requests/second per IP | None |
| Partner | 60 requests/second per API key | `X-API-Key` header |

Partner API keys are currently issued manually. Do not hard-code a key in a
browser application; use a server-side integration when authentication is
required.

Interactive OpenAPI browsing is also available in the published docs:
https://lumagg.gitbook.io/lumagg/integrate/api-reference


## Limit And DCA

Limit and DCA endpoints prepare unsigned transactions against the configured
Order Escrow contract. The wallet remains responsible for signing and
submitting the returned XDR. Listing endpoints read lifecycle events indexed in
`indexer.db_path`, so a submitted transaction may take one indexer poll to
appear. The API server must also set `ESCROW_CONTRACT`; if either setting is
missing, the listing endpoints return `503` instead of an empty result.

DCA orders divide `amount_in` into `chunk_amount` executions separated by
`interval_ledgers`. `start_ledger` cannot be in the past, and
`expires_ledger` must be later than the start and within the contract's 30-day
maximum lifetime. Set `min_out_per_in_e7` to `0` for market execution, or to a
positive rate floor for every chunk.

## Supported Liquidity

LumAgg currently routes across Soroswap, Aquarius AMM, Aquarius Stable,
Aquarius CLMM, Phoenix, Sushi V3, and Comet. Classic Stellar DEX routing is
also available for Classic-only routes and comparison. Classic and Soroban
legs are not mixed in one transaction.

For complete request and response schemas, import the repository's
[`openapi.yaml`](https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/openapi.yaml)
into Swagger UI, Postman, Insomnia, or an OpenAPI client generator.

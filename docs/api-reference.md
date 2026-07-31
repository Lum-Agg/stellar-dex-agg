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

## Core Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/v1/quote` | Find and quote a single, multi-hop, or split route |
| `POST` | `/api/v1/build_tx` | Build an unsigned transaction XDR for a quote |
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
the user's wallet signs and submits it. See the [Integrator Guide](integrator-guide.md)
for request examples, amount units, slippage, maximum hops, maximum splits,
errors, and partner API keys.

## Limit And DCA

Limit and DCA endpoints prepare unsigned transactions against the configured
Order Escrow contract. The wallet remains responsible for signing and
submitting the returned XDR. Listing endpoints read lifecycle events indexed in
`INDEXER_DB_PATH`, so a submitted transaction may take one indexer poll to
appear.

DCA orders divide `amount_in` into `chunk_amount` executions separated by
`interval_ledgers`. `start_ledger` cannot be in the past, and
`expires_ledger` must be later than the start and within the contract's 30-day
maximum lifetime. Set `min_out_per_in_e7` to `0` for market execution, or to a
positive rate floor for every chunk.

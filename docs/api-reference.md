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

The normal integration flow is:

```text
/tokens -> /quote -> /build_tx -> wallet sign -> submit to Stellar
```

LumAgg never needs the user's secret key. `/build_tx` returns unsigned XDR;
the user's wallet signs and submits it. See the [Integrator Guide](integrator-guide.md)
for request examples, amount units, slippage, maximum hops, maximum splits,
errors, and partner API keys.

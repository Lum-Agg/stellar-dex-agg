# Self-hosted Aggregator Quickstart

This quickstart runs the split LumAgg aggregator stack from release binaries:

```text
lumagg-market-data-worker -> Redis -> lumagg-api-server
```

Use this path when you want a production-like quote API with shared market
state. For a private single-process service without Redis, use
[LumAgg Swap API](lumagg-swap-api.md).

## 1. Download a release

Linux x86_64 archives are published from the monorepo:

<https://github.com/Lum-Agg/stellar-dex-agg/releases>

Download `lumagg-aggregator-linux-x86_64.tar.gz` and `SHA256SUMS`:

```bash
grep 'lumagg-aggregator-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum --check
tar -xzf lumagg-aggregator-linux-x86_64.tar.gz
cd lumagg-aggregator-linux-x86_64
```

The archive contains:

- `lumagg-market-data-worker`
- `lumagg-api-server`
- `lumagg-analytics-indexer`
- `lumagg-aggregator.toml`
- optional `systemd/` examples
- deployment and configuration docs

## 2. Prepare Redis

Run Redis on localhost or a private network. Do not expose Redis directly to the
internet.

```bash
redis-cli -u 'redis://:YOUR_PASSWORD@127.0.0.1:6379/' PING
```

If the password contains characters such as `@`, `:`, `/`, or `#`, percent
encode it in the Redis URL.

## 3. Create the config

```bash
cp lumagg-aggregator.toml aggregator.toml
chmod 600 aggregator.toml
```

Edit at least these values:

```toml
[network]
rpc_url = "https://your-soroban-rpc.example"

[redis]
url = "redis://:YOUR_PASSWORD@127.0.0.1:6379/"

[api]
listen_addr = "0.0.0.0:3100"
aggregator_contract = "YOUR_MAINNET_AGGREGATOR_CONTRACT"

[dex]
horizon_url = "https://your-horizon.example"
```

`network.rpc_url` must be a Soroban JSON-RPC endpoint. `dex.horizon_url` is used
by Classic DEX support and analytics envelope repair; use an archive-capable
Horizon if historical indexing matters.

See [Aggregator Configuration](aggregator-configuration.md) for every option.

## 4. Validate the config

```bash
./lumagg-market-data-worker --config ./aggregator.toml --check-config
./lumagg-api-server --config ./aggregator.toml --check-config
./lumagg-analytics-indexer --config ./aggregator.toml --check-config
```

Fix every validation error before starting long-running services.

## 5. Start the worker

```bash
./lumagg-market-data-worker --config ./aggregator.toml
```

Wait until it publishes the first topology snapshot to Redis. Startup time
depends on RPC capacity and enabled DEX sources.

## 6. Start the API

In another shell:

```bash
./lumagg-api-server --config ./aggregator.toml
```

Run additional API replicas with different listen ports:

```bash
./lumagg-api-server --config ./aggregator.toml --listen-addr 127.0.0.1:3101
```

## 7. Verify readiness

```bash
curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/ready
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
```

`/health` only means the process is alive. `/ready` means the API has routing
state and can answer quote requests.

## 8. Optional analytics

Start the indexer only when you need `/stats`, swap history, or arbitrage
history:

```bash
./lumagg-analytics-indexer --config ./aggregator.toml run
```

The API and indexer must point to the same `indexer.db_path`.

## Next steps

- Use [Integrator Guide](integrator-guide.md) to test `/quote` and `/build_tx`.
- Use [Production Aggregator Deployment](aggregator-deployment.md) for systemd,
  scaling, and upgrade guidance.
- Use [API Reference](api-reference.md) and [OpenAPI](openapi.yaml) for the full
  HTTP contract.

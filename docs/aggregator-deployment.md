# Production Aggregator Deployment

The scalable LumAgg topology runs native binaries as separate processes:

```text
lumagg-market-data-worker -> Redis -> lumagg-api-server x N
lumagg-analytics-indexer -> SQLite -> lumagg-api-server x N
```

Use it for a public API, high request volume, shared market state, or an
arbitrage operator. For a small private service without Redis, use
[LumAgg Swap API](lumagg-swap-api.md).

## Deployment rules

- Run exactly one `lumagg-market-data-worker` for each Redis namespace. The
  worker is the single writer and does not implement leader election.
- Run one or more stateless `lumagg-api-server` processes against that Redis.
- Keep Redis private and do not expose port 6379 to the internet.
- Use a low-latency, capacity-controlled Soroban RPC. Public rate-limited RPCs
  are suitable for evaluation, not a reliable production data plane.
- Run both binaries as an unprivileged user. Put TLS and public traffic handling
  in a reverse proxy or load balancer.

## Download

Linux x86_64 binaries are published in the monorepo releases:

<https://github.com/Lum-Agg/stellar-dex-agg/releases>

Download `lumagg-aggregator-linux-x86_64.tar.gz` and `SHA256SUMS`, then verify
and extract them:

```bash
grep 'lumagg-aggregator-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum --check
tar -xzf lumagg-aggregator-linux-x86_64.tar.gz
cd lumagg-aggregator-linux-x86_64
```

The archive contains all three binaries, a complete `lumagg-aggregator.toml`, the
configuration reference, and optional systemd units.

To build the same binaries from source instead:

```bash
git clone https://github.com/Lum-Agg/stellar-dex-agg.git
cd stellar-dex-agg
git checkout <release-tag-or-commit>
cargo build --locked --release -p market-data-worker -p api-server -p analytics-indexer
```

The Cargo package names remain `market-data-worker`, `api-server`, and
`analytics-indexer`; their release binaries use the `lumagg-` prefix.

## Configure Redis

Install Redis on the same private network as the services. Bind it to localhost
or a private interface, enable authentication, and choose persistence and
memory policies appropriate for your operation.

Verify connectivity using the URL that will be in the LumAgg configuration:

```bash
redis-cli -u 'redis://:YOUR_PASSWORD@127.0.0.1:6379/' PING
```

The password must be URL-encoded when it contains reserved URL characters.

## Configure LumAgg

Create a private runtime configuration from the complete example:

```bash
cp lumagg-aggregator.toml aggregator.toml
chmod 600 aggregator.toml
```

Replace at least:

- `network.rpc_url` with the production Soroban RPC.
- `dex.horizon_url` with the Horizon endpoint used for Classic DEX access and
  optional historical envelope fallback. Horizon and Soroban RPC may run on
  different machines or be supplied by different providers.
- `redis.url` with the private Redis URL.
- `api.aggregator_contract` with the deployed LumAgg Aggregator contract. Omit it
  only when the API should quote but never build transactions.

All processes read the same file, so network, contract, and storage settings cannot
silently diverge. Validate the file before starting either process:

```bash
./lumagg-market-data-worker --config ./aggregator.toml --check-config
./lumagg-api-server --config ./aggregator.toml --check-config
./lumagg-analytics-indexer --config ./aggregator.toml --check-config
```

See [Configuration Reference](aggregator-configuration.md)
for every supported parameter, its default, and the process that uses it.

## Run without a service manager

The binaries read the TOML file directly, so they are independent of systemd,
Docker, Kubernetes, or any other process manager:

```bash
./lumagg-market-data-worker --config ./aggregator.toml
```

After the worker publishes its first snapshot, start the API in another shell:

```bash
./lumagg-api-server --config ./aggregator.toml
```

Additional API replicas use the same config and a different `LISTEN_ADDR`:

```bash
./lumagg-api-server --config ./aggregator.toml --listen-addr 127.0.0.1:3101
```

Start the optional analytics indexer when stats and history endpoints are needed:

```bash
./lumagg-analytics-indexer --config ./aggregator.toml run
```

In production, translate the same environment file into your chosen service
manager and configure restart, resource, logging, and secret policies there.

## Verify

Liveness confirms the process is running. Readiness confirms that a routing
graph has loaded:

```bash
curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/ready
curl -fsS http://127.0.0.1:3101/api/v1/ready
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
```

Use the [Integrator Guide](integrator-guide.md) to test `/quote` and
`/build_tx`. A load balancer should route traffic only to instances whose
`/api/v1/ready` endpoint succeeds.

## Optional systemd example

The release archive includes units under `systemd/`. They are examples, not a
required deployment model. Install the binaries and config at the paths used by
those units:

```bash
sudo useradd --system --home /var/lib/lumagg --shell /usr/sbin/nologin lumagg
sudo install -d -o root -g lumagg -m 0750 /etc/lumagg
sudo install -m 0755 lumagg-market-data-worker lumagg-api-server lumagg-analytics-indexer /usr/local/bin/
sudo install -m 0640 -o root -g lumagg aggregator.toml /etc/lumagg/lumagg-aggregator.toml
sudo install -m 0644 systemd/*.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now lumagg-market-data-worker
sudo systemctl enable --now lumagg-api@3100 lumagg-api@3101
sudo systemctl enable --now lumagg-analytics-indexer
```

Inspect startup with `journalctl -u lumagg-market-data-worker -f` and
`journalctl -u 'lumagg-api@*' -f`.

The analytics indexer is the only SQLite writer. Run one indexer process for a
database file, and keep that file on local storage rather than a network
filesystem. API replicas may read the same database, but should not run an
additional indexer against it.

## Scale and upgrade

API capacity scales horizontally by adding `lumagg-api-server` processes or
hosts connected to the same private Redis. Do not add a second worker to the
same namespace unless an external active/passive mechanism guarantees only one
is running.

For upgrades, retain the previous binaries for rollback, replace both binaries,
then restart the worker followed by API replicas. Confirm worker publication
and every API readiness endpoint before returning traffic to all replicas.

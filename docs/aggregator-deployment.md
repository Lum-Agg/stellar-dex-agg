# Production Aggregator Deployment

This guide deploys the scalable LumAgg topology as separate native processes:

```text
market-data-worker -> Redis -> api-server x N
```

Use this topology for a public API, high request volume, shared market state, or
an arbitrage operator. For a small private service without Redis, use
[LumAgg Swap API](lumagg-swap-api.md) instead.

## Deployment Rules

- Run exactly one `market-data-worker` for each Redis namespace. The worker is
  the single writer and does not implement leader election.
- Run one or more stateless `api-server` instances against the same Redis.
- Keep Redis private. Do not expose port 6379 to the internet.
- Use a low-latency, capacity-controlled Soroban RPC. Public rate-limited RPCs
  are suitable for evaluation, not a reliable production data plane.
- Run the services as an unprivileged user and expose API instances through a
  reverse proxy or load balancer.

## Build

Install Rust and the native build dependencies required by the workspace, then
build from a tagged revision:

```bash
git clone https://github.com/Lum-Agg/stellar-dex-agg.git
cd stellar-dex-agg
git checkout <release-tag-or-commit>
cargo build --locked --release -p market-data-worker -p api-server
```

Create the service account and install the binaries:

```bash
sudo useradd --system --home /var/lib/lumagg --shell /usr/sbin/nologin lumagg
sudo install -d -o lumagg -g lumagg -m 0750 /var/lib/lumagg/logos
sudo install -d -o root -g lumagg -m 0750 /etc/lumagg
sudo install -m 0755 target/release/market-data-worker /usr/local/bin/
sudo install -m 0755 target/release/api-server /usr/local/bin/
sudo cp -R data/logos/. /var/lib/lumagg/logos/
sudo chown -R lumagg:lumagg /var/lib/lumagg/logos
```

If the `lumagg` account already exists, `useradd` can be skipped.

## Configure Redis

Install Redis on the same private network as the services. Bind it to localhost
or a private interface, enable authentication, and configure persistence and
memory policy for your operational requirements.

Before starting LumAgg, verify connectivity using the same URL that will be in
the environment file:

```bash
redis-cli -u 'redis://:YOUR_PASSWORD@127.0.0.1:6379/' PING
```

The password must be URL-encoded when it contains reserved URL characters.

## Configure LumAgg

Install the public environment and systemd templates:

```bash
sudo install -m 0640 -o root -g lumagg \
  packaging/aggregator.env.example /etc/lumagg/aggregator.env
sudo install -m 0644 packaging/lumagg-market-data-worker.service \
  /etc/systemd/system/
sudo install -m 0644 packaging/lumagg-api@.service \
  /etc/systemd/system/
```

Edit `/etc/lumagg/aggregator.env` and replace at least:

- `RPC_URL` with the production Soroban RPC.
- `SNAPSHOT_REDIS_URL` with the private Redis URL.
- `AGGREGATOR_CONTRACT` with the deployed LumAgg Aggregator contract. Remove
  this variable if the service should only quote and never build transactions.

Keep `SNAPSHOT_REDIS_CHANNEL` identical across the worker and all API replicas.
The example values for routing and concurrency are starting points, not
capacity guarantees.

## Start

Start Redis first, then the single worker, then API instances:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now lumagg-market-data-worker
sudo journalctl -u lumagg-market-data-worker -f
```

Initial discovery can take time. After the worker has published a snapshot,
start one or more API ports:

```bash
sudo systemctl enable --now lumagg-api@3100 lumagg-api@3101
```

The template binds each instance to `127.0.0.1:<port>`. Put TLS and public
traffic handling in a reverse proxy rather than changing the processes to run
as root.

## Verify

Liveness only confirms that the process is running. Readiness confirms that a
routing graph has loaded:

```bash
curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/ready
curl -fsS http://127.0.0.1:3101/api/v1/ready
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
```

Use the [Integrator Guide](integrator-guide.md) to test `/quote` and
`/build_tx`. A load balancer should route traffic only to instances whose
`/api/v1/ready` endpoint succeeds.

## Scale and Upgrade

API capacity scales horizontally by adding `lumagg-api@<port>` instances or
hosts connected to the same private Redis. Do not add a second worker to the
same namespace unless an external active/passive mechanism guarantees that
only one is running.

For an upgrade, build the exact target revision with `--locked`, retain the
previous binaries for rollback, replace the installed binaries, and restart
the worker followed by API instances. Check worker publication and every API
readiness endpoint before returning all replicas to the load balancer.

Useful logs:

```bash
journalctl -u lumagg-market-data-worker -f
journalctl -u 'lumagg-api@*' -f
```

Common readiness failures are an unreachable RPC, Redis authentication errors,
or an API configured with a different Redis URL or event channel than the
worker.

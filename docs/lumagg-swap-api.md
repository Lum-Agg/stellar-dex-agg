# LumAgg Swap API

`lumagg-swap-api` is the self-contained LumAgg quote service. It runs the REST
API and market-data worker in one process with in-memory stores, so Redis is not
required.

Use it for local development, integration testing, a private quote service, or
as the quote service beside an arbitrage operator. It is not the recommended
topology for a horizontally scaled public aggregator.

## Download

Linux x86_64 binaries are published from the monorepo under tags named
`swap-api-v*`:

<https://github.com/Lum-Agg/stellar-dex-agg/releases>

Download the archive and `SHA256SUMS`, then verify and extract it:

```bash
grep 'lumagg-swap-api-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum --check
tar -xzf lumagg-swap-api-linux-x86_64.tar.gz
cd lumagg-swap-api-linux-x86_64
```

## Run

At minimum, configure a reachable Stellar Soroban RPC:

```bash
export RPC_URL=https://your-stellar-rpc.example.com
export LISTEN_ADDR=127.0.0.1:3100
./lumagg-swap-api
```

The process accepts HTTP connections before initial market discovery is
complete. Wait for readiness before requesting quotes:

```bash
until curl -fsS http://127.0.0.1:3100/api/v1/ready; do sleep 2; done

curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
```

The release archive includes `lumagg-swap-api.env.example` with the common
settings. The complete API contract is in
[`docs/openapi.yaml`](https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/openapi.yaml).

## Run with systemd

The release archive includes a hardened `lumagg-swap-api.service` template:

```bash
sudo useradd --system --home /var/lib/lumagg --shell /usr/sbin/nologin lumagg
sudo install -m 0755 lumagg-swap-api /usr/local/bin/lumagg-swap-api
sudo install -d -m 0750 /etc/lumagg
sudo install -m 0640 lumagg-swap-api.env.example /etc/lumagg/lumagg-swap-api.env
sudo install -m 0644 lumagg-swap-api.service /etc/systemd/system/lumagg-swap-api.service
sudo systemctl daemon-reload
sudo systemctl enable --now lumagg-swap-api
```

Edit `/etc/lumagg/lumagg-swap-api.env` before starting the service. Check
startup and readiness with:

```bash
systemctl status lumagg-swap-api
journalctl -u lumagg-swap-api -f
curl -i http://127.0.0.1:3100/api/v1/ready
```

## Build from source

```bash
cargo build --locked --release -p lumagg-swap-api
./target/release/lumagg-swap-api --version
```

## Production aggregator topology

LumAgg's production deployment keeps components separate:

```text
lumagg-market-data-worker -> Redis -> lumagg-api-server x N
```

Run `lumagg-market-data-worker` and `lumagg-api-server` independently
when API horizontal scaling, failure isolation, or shared market state is
required. The all-in-one binary does not replace this topology.

## Arbitrage boundary

`lumagg-swap-api` does not execute arbitrage. Run `lumagg-arbitrage-bot` separately
and point `ARB_QUOTE_API_URL` or `ARB_QUOTE_API_URLS` at this service. For a
high-throughput mainnet operator, use the production aggregator topology above.

## Operational notes

- Market state is held in memory and is rebuilt after every restart.
- `/api/v1/health` is process liveness; `/api/v1/ready` is routing readiness.
- The embedded worker restarts with bounded exponential backoff after an RPC
  startup failure; readiness remains false until a snapshot is published.
- RPC capacity directly limits discovery, refresh, quote preparation, and
  transaction simulation throughput.
- Keep the API and on-chain Aggregator contract versions compatible when
  overriding `AGGREGATOR_CONTRACT`.

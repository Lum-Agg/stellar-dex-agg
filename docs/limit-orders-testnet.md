# Limit orders — testnet deploy & smoke

**Network:** Stellar **testnet only**. Do not point these scripts at mainnet.

Scripts refuse `Public Global Stellar Network` (mainnet) passphrases.

## Prerequisites

- [stellar CLI](https://developers.stellar.org/docs/tools/cli)
- Funded testnet identity (Friendbot) registered locally, e.g. `stellar keys add admin --secret-key …`
- Working directory: repo root

## Deploy

One shot (aggregator + escrow + env file):

```bash
ADMIN=admin ADMIN_G=G... ./scripts/deploy-limit-testnet.sh
```

Or stepwise:

```bash
ADMIN=admin ADMIN_G=G... ./contracts/aggregator/deploy-testnet.sh
# optional: AGGREGATOR=C... to reuse an existing testnet aggregator
AGGREGATOR=C... ADMIN=admin ADMIN_G=G... ./contracts/order-escrow/deploy-testnet.sh
```

Artifacts (gitignored):

| Path | Contents |
|------|----------|
| `contracts/aggregator/.testnet-aggregator-id` | Aggregator `C…` |
| `contracts/order-escrow/.testnet-escrow-id` | Escrow `C…` |
| `deploy/.env.limit-testnet.local` | Env for API / indexer / keeper |

Default RPC: `https://soroban-testnet.stellar.org`  
Passphrase: `Test SDF Network ; September 2015`

## Point services at testnet

The preferred keeper deployment uses a TOML file and a separate secret file:

```bash
cp packaging/lumagg-limit-keeper.toml limit-keeper-testnet.toml
sed -i 's/network = "public"/network = "testnet"/' limit-keeper-testnet.toml
sed -i 's#https://your-stellar-rpc.example.com#https://soroban-testnet.stellar.org#' limit-keeper-testnet.toml
# Set escrow_contract, aggregator_contract, quote_api_url, and cursor_path.
./target/release/limit-keeper --config limit-keeper-testnet.toml
```

Use `dry_run = true` while validating discovery and fillability. For live testnet
fills, set `dry_run = false`, create the file referenced by `secret_file` with
mode `0600`, and keep it outside Git. The environment-variable form below is
retained for compatibility with the existing testnet deployment scripts.

```bash
set -a && source deploy/.env.limit-testnet.local && set +a

# lumagg-analytics-indexer (polls escrow events into indexer.db_path)
# api-server (GET /orders + build_create/build_cancel use indexer.db_path + features.escrow_contract)
# limit-keeper (KEEPER_NETWORK=testnet, ESCROW_CONTRACT, AGGREGATOR_CONTRACT)
```

Use **testnet** token contract ids and a user account that exists on testnet for builds.

On the server, copy the generated env to
`/opt/stellar-dex-aggregator/deploy/.env.limit-testnet.local`, then install and
start the API and indexer units. The keeper starts in dry-run mode by default:

```bash
RESET_TESTNET_DB=1 ./deploy_limit_testnet_server.sh
```

The deployment uses separate `*-testnet` binary names, a Soroswap testnet
worker, and Redis DB 15. It does not replace the running mainnet API, worker, or
indexer binaries. Before the first deployment, create the server-only Redis
configuration:

```bash
sudo install -d -m 700 /etc/lumagg
sudo sh -c 'umask 077; printf "%s\n" \
  "SNAPSHOT_REDIS_URL=redis://default:PASSWORD@127.0.0.1:6379/15" \
  > /etc/lumagg/testnet-redis.env'
```

`RESET_TESTNET_DB=1` backs up the old testnet database and keeper cursor before
starting from the newly deployed Escrow. For manual installation:

```bash
sudo install -m 644 deploy/lumagg-api-testnet.service /etc/systemd/system/
sudo install -m 644 deploy/lumagg-indexer-testnet.service /etc/systemd/system/
sudo install -m 644 deploy/lumagg-limit-keeper-testnet.service /etc/systemd/system/
sudo install -m 644 deploy/lumagg-worker-testnet.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now lumagg-worker-testnet lumagg-api-testnet lumagg-indexer-testnet
sudo systemctl enable --now lumagg-limit-keeper-testnet
```

For live fills, place the funded keeper seed only on the server:

```bash
sudo install -d -m 700 /etc/lumagg
sudo sh -c 'umask 077; printf "%s\n" \
  "KEEPER_SECRET=S..." \
  "KEEPER_DRY_RUN=0" \
  > /etc/lumagg/limit-keeper-testnet.env'
sudo systemctl restart lumagg-limit-keeper-testnet
```

Never add `KEEPER_SECRET` to the generated shared env or repository.

## Smoke checklist

| # | Step | Expect |
|---|------|--------|
| 1 | Deploy scripts complete | Two `C…` ids; explorer links under `/testnet/` |
| 2 | Start indexer + api-server with env snippet | No mainnet defaults |
| 3 | `POST /api/v1/orders/build_create` with testnet user/tokens | `unsigned_tx_xdr` |
| 4 | Sign + submit on testnet | `order_created` on escrow |
| 5 | Wait for indexer poll → `GET /api/v1/orders?user=G…` | Open order listed |
| 6 | Optional: `POST .../build_cancel` or keeper dry-run | Cancel XDR / dry-run fill log |
| 7 | Create DCA and run keeper after its due ledger | One chunk fills and next ledger advances |

**Verified live fill (2026-07-31):**

- Escrow: `CDCNJOKHKC7HG5A46RKG7QSNBIU3ES2A5PFOYPORJTUQKP4WXMX4OFD6`
- Aggregator: `CDJI26DXFQ4MD7VICA3Q6NEGWF53A3Z6IK7WTNMQ6UZUHL5XGQMEKJRE`
- DCA fill transaction:
  [`64f23f734397e88c17fc57dee91dcfdec7636c9a1a00f5a363ef6aa7657b689c`](https://stellar.expert/explorer/testnet/tx/64f23f734397e88c17fc57dee91dcfdec7636c9a1a00f5a363ef6aa7657b689c)
- Route: XLM → XTAR → test-USDC across two Soroswap pairs
- Result: `500000` input, `326666` output, `500000` remaining; the remainder was
  cancelled and refunded in transaction
  [`ac487d9f416f19c34a449cf6e8114eeb6e3a98ac1fa080b66e76492a37ef93eb`](https://stellar.expert/explorer/testnet/tx/ac487d9f416f19c34a449cf6e8114eeb6e3a98ac1fa080b66e76492a37ef93eb).
- Post-test balances: Escrow XLM/test-USDC = `0`/`0`; Aggregator
  XLM/test-USDC = `0`/`0`.

## Out of scope

- Mainnet deploy of aggregator/escrow for limits  
- Changing production `api.lumagg.xyz`

## Frontend (Phase 3d)

On `/`, switch Order rail to **Limit** or **DCA**. DCA supports a total amount,
fixed chunk, hourly/6-hour/daily frequency, and an optional minimum execution
price. Its API surface is `/api/v1/dca`, `/dca/build_create`, and
`/dca/build_cancel` under the same `/api/v1` prefix.

| Piece | Value |
|-------|--------|
| Frontend env | `NEXT_PUBLIC_LIMIT_API_URL=https://api.lumagg.xyz/limit-testnet` |
| Nginx | `api.lumagg.xyz/limit-testnet/` → `127.0.0.1:3200` |
| systemd | `lumagg-worker-testnet`, `lumagg-api-testnet`, `lumagg-indexer-testnet`, `lumagg-limit-keeper-testnet` |
| Escrow | Read from `deploy/.env.limit-testnet.local` after each deployment |

Wallet must be on **Testnet** when signing create/cancel. Instant still uses `NEXT_PUBLIC_API_URL` (mainnet).

Local: `packages/frontend/.env.local`. Deploy UI: `./deploy_site.sh`.

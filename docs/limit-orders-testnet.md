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

```bash
set -a && source deploy/.env.limit-testnet.local && set +a

# analytics-indexer (polls escrow events into INDEXER_DB_PATH)
# api-server (GET /orders + build_create/build_cancel need INDEXER_DB_PATH + ESCROW_CONTRACT)
# limit-keeper (KEEPER_NETWORK=testnet, ESCROW_CONTRACT, AGGREGATOR_CONTRACT)
```

Use **testnet** token contract ids and a user account that exists on testnet for builds.

## Smoke checklist

| # | Step | Expect |
|---|------|--------|
| 1 | Deploy scripts complete | Two `C…` ids; explorer links under `/testnet/` |
| 2 | Start indexer + api-server with env snippet | No mainnet defaults |
| 3 | `POST /api/v1/orders/build_create` with testnet user/tokens | `unsigned_tx_xdr` |
| 4 | Sign + submit on testnet | `order_created` on escrow |
| 5 | Wait for indexer poll → `GET /api/v1/orders?user=G…` | Open order listed |
| 6 | Optional: `POST .../build_cancel` or keeper dry-run | Cancel XDR / dry-run fill log |

**Fill / live keeper:** testnet DEX liquidity may be thin. Create/cancel/list is enough to validate custody + API; a successful market fill is best-effort.

## Out of scope

- Mainnet deploy of aggregator/escrow for limits  
- Phase 3d Limit UI  
- Changing production `api.lumagg.xyz`  

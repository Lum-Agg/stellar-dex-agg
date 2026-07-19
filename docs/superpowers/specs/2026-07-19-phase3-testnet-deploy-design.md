# LumAgg: Testnet Deploy for Limit Orders (Aggregator + Escrow)

**Date:** 2026-07-19  
**Status:** Approved for planning  
**Depends on:** order-escrow (3a), limit-keeper (3b), orders API (3c)

## Goal

Deploy **aggregator** and **order-escrow** on **Stellar testnet only**, initialize escrow against that aggregator, and document env + smoke steps so API / indexer / keeper can be pointed at the deployment. No UI, no mainnet.

## Hard constraints

| Rule | Detail |
|------|--------|
| Network | **Testnet only** — default passphrase `Test SDF Network ; September 2015`, default RPC `https://soroban-testnet.stellar.org` |
| Mainnet | **Forbidden** for these scripts — refuse if `NETWORK_PASSPHRASE` contains `Public Global Stellar Network` (or equivalent public mainnet marker) |
| Secrets | Deploy uses local stellar CLI identity; no mainnet keys; do not commit private keys or funded secrets |
| Scope | Scripts + docs + env template; not production `deploy_server.sh` cutover |

## Deliverables

### 1. Aggregator testnet deploy script

Path (proposed): `contracts/aggregator/deploy-testnet.sh`

- Build/optimize aggregator WASM (same patterns as `upgrade.sh` / vault `deploy.sh`)
- `stellar contract deploy` → capture `C…` id
- `initialize(--admin ADMIN_G)` once after deploy
- Optional: `AGGREGATOR=C…` skip deploy and reuse an existing testnet aggregator (still validate network is testnet)
- Save id to a **gitignored** local file e.g. `contracts/aggregator/.testnet-aggregator-id`
- Print explorer links for **testnet** (not public mainnet)

### 2. Order-escrow testnet deploy script

Path (proposed): `contracts/order-escrow/deploy-testnet.sh`

- Build/optimize `order-escrow` WASM
- Deploy → capture escrow id
- `initialize(--admin ADMIN_G --aggregator AGGREGATOR)` where aggregator comes from arg/env or aggregator script output
- Save id to `contracts/order-escrow/.testnet-escrow-id` (gitignored)
- Same mainnet-refuse guard as aggregator script

### 3. Convenience wrapper (optional but recommended)

Path (proposed): `scripts/deploy-limit-testnet.sh`

- Runs aggregator deploy (or reuse), then escrow deploy + initialize
- Writes a single **env snippet** file (gitignored), e.g. `deploy/.env.limit-testnet.local`:

```bash
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
# or RPC_URL / INDEXER_RPC_URL / KEEPER_RPC_URL aliases as needed
NETWORK_PASSPHRASE=Test SDF Network ; September 2015
KEEPER_NETWORK=testnet
AGGREGATOR_CONTRACT=C...
ESCROW_CONTRACT=C...
INDEXER_DB_PATH=./data/analytics-indexer-testnet.db
```

### 4. Operator / smoke doc

Path (proposed): `docs/limit-orders-testnet.md`

Checklist:

1. Prerequisites: stellar CLI, funded testnet identity (`ADMIN`), Friendbot if needed  
2. Run deploy scripts → record both contract ids  
3. Point local api-server + analytics-indexer at testnet RPC + `ESCROW_CONTRACT` + `AGGREGATOR_CONTRACT` + shared `INDEXER_DB_PATH`  
4. Smoke: `POST /orders/build_create` → wallet sign/submit → indexer catches `order_created` → `GET /orders?user=`  
5. Optional: `build_cancel` or keeper dry-run against open order  
6. Explicit note: **do not** point these scripts at mainnet; production later is a separate decision

## Non-goals

- Mainnet deploy of aggregator/escrow for limit orders  
- Phase 3d Limit UI  
- DCA  
- Changing live `api.lumagg.xyz` / mainnet indexer  
- Automated CI that submits funded txs (manual smoke only)  
- Wiring production DEX pool registry for testnet liquidity (fill smoke may need known testnet pools; document gap if thin liquidity)

## Acceptance

1. Fresh clone + funded testnet key can run scripts and obtain two `C…` ids on testnet  
2. Escrow `initialize` succeeds with those ids  
3. Scripts exit non-zero if asked to use mainnet passphrase  
4. Doc + env snippet suffice to start indexer/API/keeper against the deployment  
5. No mainnet contract ids written as defaults in the new scripts  

## Risks / notes

- Testnet DEX liquidity may be sparse — create/cancel/list can be proven without a successful market fill; fill/keeper live fill is best-effort  
- Aggregator admin/upgrade path on testnet is independent of mainnet aggregator id `CC6Q…`  
- `.testnet-*-id` and local env files must stay gitignored  

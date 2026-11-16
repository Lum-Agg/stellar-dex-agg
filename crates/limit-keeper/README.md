# limit-keeper

Self-hosted operator binary that discovers open limit orders on the LumAgg
`order-escrow` contract via Soroban `getEvents`, quotes through the LumAgg API,
and submits `fill` with the configured on-chain keeper's authorization when
the limit price is met.

The keeper also executes due DCA chunks. Limit and DCA IDs use separate
namespaces and are tracked as `(order_kind, order_id)` to avoid collisions.

## Environment

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `KEEPER_RPC_URL` | yes | — | Soroban RPC endpoint |
| `KEEPER_SECRET` | yes* | — | Filler account secret seed (signs fill txs) |
| `KEEPER_NETWORK` | yes | — | Network passphrase (`public` or `testnet`) |
| `ESCROW_CONTRACT` | yes | — | Deployed `order-escrow` contract ID (`C…`) |
| `AGGREGATOR_CONTRACT` | yes | — | LumAgg aggregator contract ID (`C…`) |
| `QUOTE_API_URL` | yes | — | LumAgg quote API base URL (e.g. `https://api.lumagg.xyz`) |
| `KEEPER_POLL_SECS` | no | `10` | Seconds between poll loops |
| `KEEPER_CURSOR_PATH` | no | `keeper.cursor` | Base checkpoint filename; the keeper adds the network and escrow ID |
| `KEEPER_START_LEDGER` | no | recent window | First ledger to scan when no scoped checkpoint exists |
| `KEEPER_DRY_RUN` | no | off | Set to `1` to quote and log only; never sign or submit |
| `KEEPER_MAX_FILL` | no | — | Optional cap per fill (stroops) |
| `KEEPER_RECLAIM` | no | off | when `1`, submit permissionless reclaim transactions for expired Limit/DCA orders |

\* `KEEPER_SECRET` is optional when `KEEPER_DRY_RUN=1` (dry-run never signs).

Checkpoints are JSON files named `<base>.<network>.<escrow>.json` and include
both the cursor and every open order. Unscoped files are not imported. On a
fresh deployment use its deployment ledger (or an earlier ledger) as the
starting point. If the cursor is older than the RPC's retained history, the
keeper pauses fills and automatic reclaims without changing the checkpoint.
Point it at an RPC on the same network that still serves the cursor ledger
and restart it with the same checkpoint. Do not advance the cursor to hide
the gap: that can omit open orders.
An incomplete page sequence or malformed order event also pauses automatic
operations without advancing past the affected ledger range.

Deploy escrow + aggregator on **testnet**: [docs/limit-orders-testnet.md](../../docs/limit-orders-testnet.md)
(`scripts/deploy-limit-testnet.sh`). Do not use those scripts on mainnet.

## Dry-run example

Validate discovery, quoting, and fillability without spending fees or submitting
transactions:

```bash
export KEEPER_RPC_URL=https://soroban-testnet.stellar.org
export KEEPER_NETWORK=testnet
export ESCROW_CONTRACT=C…
export AGGREGATOR_CONTRACT=C…
export QUOTE_API_URL=https://api.lumagg.xyz
export KEEPER_DRY_RUN=1
export KEEPER_POLL_SECS=15

cargo run -p limit-keeper
```

With dry-run enabled, the keeper polls escrow events, maintains an open order
book backed by the atomic checkpoint, fetches quotes for executable Limit and
due DCA candidates, and logs lines like `dry-run: would fill escrow order`
instead of calling `execute_fill`.

## Live operation

Unset `KEEPER_DRY_RUN` (or set it to anything other than `1` / `true`) and provide
`KEEPER_SECRET` for an account funded with XLM for fees:

```bash
export KEEPER_SECRET=S…
unset KEEPER_DRY_RUN

cargo run -p limit-keeper --release
```

## Reclaim

`KEEPER_RECLAIM=1` enables permissionless reclaim transactions for orders whose
expiry ledger has passed. Limit orders use `reclaim_expired`; DCA orders use
`reclaim_expired_dca`. The keeper waits until the expiry ledger is strictly in
the past and removes an order from its local book only after confirmation.

## Build & test

```bash
cargo test -p limit-keeper
cargo test -p order-escrow-contract
```

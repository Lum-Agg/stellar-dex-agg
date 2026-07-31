# limit-keeper

Self-hosted operator binary that discovers open limit orders on the LumAgg
`order-escrow` contract via Soroban `getEvents`, quotes through the LumAgg API,
and permissionlessly submits `fill` when the on-chain limit price is met.

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
| `KEEPER_CURSOR_PATH` | no | `keeper.cursor` | Atomic checkpoint containing the ledger cursor and open orders |
| `KEEPER_DRY_RUN` | no | off | Set to `1` to quote and log only; never sign or submit |
| `KEEPER_MAX_FILL` | no | — | Optional cap per fill (stroops) |
| `KEEPER_RECLAIM` | no | off | **MVP skip:** when `1`, expired orders are logged but reclaim txs are not submitted |

\* `KEEPER_SECRET` is optional when `KEEPER_DRY_RUN=1` (dry-run never signs).

The checkpoint format is JSON and includes both the cursor and every open
order. A legacy cursor-only file is rejected instead of silently starting with
an empty order book. Remove/rebuild the old checkpoint only after confirming
there are no open orders, or replay escrow events from before the oldest open
order.

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

## Reclaim (MVP)

`KEEPER_RECLAIM=1` is recognized in config but **not implemented** in this MVP:
expired orders are skipped and no `reclaim_expired` transaction is built or
submitted. Run reclaim manually or wait for a follow-up slice if needed.

## Build & test

```bash
cargo test -p limit-keeper
cargo test -p order-escrow-contract
```

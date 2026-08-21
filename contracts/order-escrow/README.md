# Order escrow

`order-escrow` holds a user's input token for a limit order and permits anyone
to fill it through the aggregator's restricted route. The contract is the custodian of the
unfilled input; it is not an order book or a price oracle.

## ABI

### `initialize(admin, aggregator)`

Initializes the contract once. `admin` must authorize initialization.
`aggregator` is the deployed LumAgg aggregator contract used by fills.

### `upgrade(new_wasm_hash)`

Upgrades the escrow WASM while preserving the existing contract address and
orders. Only the configured admin may call it. Upgrade the Aggregator first
when the new escrow depends on a new Aggregator entrypoint such as
`swap_restricted`.

### `create_limit(owner, token_in, token_out, amount_in, limit_out_per_in_e7, expires_ledger) -> order_id`

Creates an open limit order and transfers `amount_in` from `owner` into this
contract. `owner` must authorize. The amount and limit must be positive, the
tokens must differ, and `expires_ledger` must be later than the current ledger.

The created order stores its owner, pair, remaining input, rate limit, expiry,
and status (`Open`, `Filled`, `Cancelled`, or `Expired`).

Fills use `aggregator.swap_restricted`, not the public open-routing `swap`
entrypoint. Every `(DexType, dex_id)` in a limit/DCA route must first be
registered by the aggregator administrator with `set_venue`; this prevents a
permissionless filler from substituting an arbitrary ABI-compatible contract.
Public `swap` remains open-routed for ordinary user-directed swaps.

### `fill(order_id, amount_in, sub_routes, min_amount_out) -> amount_out`

Fills all or part of an open, unexpired order. This entrypoint is
permissionless: callers do not need the owner's authorization.

`amount_in` must be positive and no greater than the remaining input. The sum
of all `sub_routes[].amount_in` values must equal `amount_in`, and
`min_amount_out` must meet the stored limit. The contract calls
`aggregator.swap_restricted`, transfers the resulting output to the order owner, reduces
the remaining input, and sets the status to `Filled` once no input remains.

### `cancel(order_id)`

Cancels an open order. Only the owner may call it. All remaining `token_in` is
returned to the owner and the order status becomes `Cancelled`.

### `reclaim_expired(order_id)`

Refunds the remaining `token_in` from an open order after its expiry. Anyone
may call it once `current_ledger > expires_ledger`; no caller authorization is
required. The order becomes `Expired`, so it cannot be reclaimed or filled
again.

## Limit-rate math

`limit_out_per_in_e7` represents the minimum output stroops per input stroop,
scaled by `10_000_000`. For each fill:

```text
required_min_out = floor(amount_in * limit_out_per_in_e7 / 10_000_000)
```

For example, a rate of `20_000_000` means at least two output stroops for each
input stroop. A partial fill of `2_500_000` input stroops requires a
`min_amount_out` of at least `5_000_000`. Computing this per fill preserves
the user's rate across partial fills.

## Aggregator authorization decision

The escrow contract is passed as `user` to `aggregator.swap_restricted`. A direct contract
call made by the current contract is already authorized by Soroban, so
`env.authorize_as_current_contract` must authorize only the deeper call made by
the aggregator:

`token_in.transfer(escrow, aggregator, total_input)`

Do not wrap this transfer under an `aggregator.swap_restricted` authorization entry. That
shape is rejected on-chain because `swap` is already in the invocation stack.
The fill tests clear all external auth mocks before execution so this contract
authorization cannot be hidden by permissive test helpers.

## Testnet deploy

**Testnet only** (scripts refuse mainnet):

```bash
ADMIN=admin ADMIN_G=G... ./scripts/deploy-limit-testnet.sh
# or: ./contracts/order-escrow/deploy-testnet.sh  # requires AGGREGATOR=C...
```

Smoke checklist: [docs/limit-orders-testnet.md](../../docs/limit-orders-testnet.md).

To upgrade an existing testnet escrow without changing its address:

```bash
ESCROW=C... ADMIN=admin ./contracts/order-escrow/upgrade-testnet.sh
```

## DCA orders

`create_dca` locks a total input amount and stores a chunk size, interval,
next executable ledger, optional E7 price floor, and expiry. A zero price floor
means market execution. `fill_dca` is permissionless and executes at most one
chunk after the due ledger; the final fill uses the remaining amount when it is
smaller than the configured chunk.

After a fill, the next due ledger is `current + interval`, preventing repeated
catch-up fills in one ledger. `cancel_dca` requires owner authorization and
refunds unspent input; `reclaim_expired_dca` is permissionless but always sends
the refund to the owner. DCA orders share the 30-day lifetime ceiling.

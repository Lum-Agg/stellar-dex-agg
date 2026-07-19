# Order escrow

`order-escrow` holds a user's input token for a limit order and permits anyone
to fill it through `aggregator.swap`. The contract is the custodian of the
unfilled input; it is not an order book or a price oracle.

## ABI

### `initialize(admin, aggregator)`

Initializes the contract once. `admin` must authorize initialization.
`aggregator` is the deployed LumAgg aggregator contract used by fills.

### `create_limit(owner, token_in, token_out, amount_in, limit_out_per_in_e7, expires_ledger) -> order_id`

Creates an open limit order and transfers `amount_in` from `owner` into this
contract. `owner` must authorize. The amount and limit must be positive, the
tokens must differ, and `expires_ledger` must be later than the current ledger.

The created order stores its owner, pair, remaining input, rate limit, expiry,
and status (`Open`, `Filled`, `Cancelled`, or `Expired`).

### `fill(order_id, amount_in, sub_routes, min_amount_out) -> amount_out`

Fills all or part of an open, unexpired order. This entrypoint is
permissionless: callers do not need the owner's authorization.

`amount_in` must be positive and no greater than the remaining input. The sum
of all `sub_routes[].amount_in` values must equal `amount_in`, and
`min_amount_out` must meet the stored limit. The contract calls
`aggregator.swap`, transfers the resulting output to the order owner, reduces
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

The escrow contract is passed as `user` to `aggregator.swap`. Before the call,
it uses `env.authorize_as_current_contract` to authorize the exact nested call
tree:

1. `aggregator.swap(user = escrow, ...)`
2. the aggregator's `token_in.transfer(escrow, aggregator, total_input)`

The authorization spike executes a 1:1 mock Aquarius route and verifies that
the aggregator can pull input from escrow and return output to it. Therefore,
the aggregator does not need a special `swap_from` entrypoint or other change.

## DCA and off-chain services

DCA is out of scope for this contract slice. It will reuse escrow custody with
schedule fields and chunked fills in a later phase. This crate also contains no
keeper, API, indexer, or UI: those services are separate follow-on work.

# Order escrow

Task 1 scaffolds the contract and proves its aggregator authorization path; no
limit-order ABI is implemented yet.

## Aggregator authorization decision

`OrderEscrowContract` is the `user` passed to `aggregator.swap`. Before making
that call, escrow uses `env.authorize_as_current_contract` to authorize the
exact nested invocation tree:

1. `aggregator.swap(user = escrow, ...)`
2. the aggregator's `token_in.transfer(escrow, aggregator, total_input)`

The `spike_escrow_can_be_aggregator_user` test executes a 1:1 mock Aquarius
route and confirms that the output returns to escrow. No aggregator change is
required.

The test uses `mock_all_auths_allowing_non_root_auth`, which is required to
exercise contract-originated authorization in Soroban testutils.

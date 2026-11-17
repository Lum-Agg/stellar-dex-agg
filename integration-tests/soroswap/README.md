# Soroswap integration regressions

These tests build the aggregator from this checkout and execute its WASM against
captured mainnet Soroswap contracts using [Kanatoko](https://crates.io/crates/kanatoko)
29.1.2 from crates.io. The replay workspace is separate from the production workspace.

## Run

Install Rust 1.95.0 with the `wasm32v1-none` target, then run from the repository root:

```sh
rustup toolchain install 1.95.0 --profile minimal --target wasm32v1-none
./integration-tests/soroswap/check.sh
```

The script rebuilds the current aggregator and runs both tests. Cargo may download
dependencies on first use; the tests themselves load committed fixtures and do not
contact RPC. After dependencies are cached, `check.sh --offline` also makes Cargo
operate offline. No CLI wallet, signing key or transaction submission is needed.

## Coverage

- Two split legs share the same real XLM/USDC pair. The second leg sees the first
  leg's reserve changes, so the combined output is less than twice an isolated
  quote. Trader, aggregator and pair balances settle exactly, including pair reserves.
- An aggregate minimum one stroop above the real router's sequential quote rejects
  with `Contract(8)`. All checked balances and reserves remain unchanged. Retrying
  with the exact minimum succeeds and matches that quote.

Each leg is 1% of the captured XLM reserve, rounded down to a multiple of 1000 to
avoid fractional fee rounding. The expected output uses the real router's
`router_get_amount_out`, with reserves advanced between legs.

## Frozen state

| Fixture | Mainnet ledger | Protocol |
| --- | --- | --- |
| `same-pair-split.json` | 65534382 | 29 |
| `minimum-rollback.json` | 65534377 | 29 |

Both captures were taken on 2026-11-17 UTC. They contain deployed Soroswap code,
the required ledger entries, proven absences and capture provenance. The router
is `CAG5LRYQ5JVEUI5TEID72EYOVX44TTUJT5BQR2J6J77FH65PCCFAJDDH`; the pair is resolved
through its `router_pair_for`. Each capture reports zero RPC reads during final replay.

The aggregator WASM is rebuilt, ignored by Git and never loaded from a deployed
aggregator address. In CI the native contract tests run before this replay suite.

For an intentional fixture refresh, first build the contract, then run from this
directory with `KANATOKO_CAPTURE_RPC=<mainnet-rpc-url> cargo +1.95.0 test --locked -- --test-threads=1`.
That mode reads live state and replaces the fixtures; review their changes before
committing. Normal `check.sh` clears this variable and always uses frozen state.

Authorization is mocked and accounts are funded locally. These tests cover the
swap results and selected rollback state; they do not validate signatures, network
admission, fees, every storage key or the off-chain route optimizer.

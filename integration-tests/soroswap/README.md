# Two legs sharing one Soroswap pair

If both legs of a split trade use the same XLM/USDC pair, the first leg moves the reserves seen by the second. [Kanatoko](https://crates.io/crates/kanatoko) runs the aggregator built from this checkout against saved Soroswap code and reserves, so both legs trade in the same pool.

| On the captured pair | USDC output |
| --- | ---: |
| One 86.1415000 XLM leg, quoted in isolation | 11.9654142 |
| Two such legs, both quoted at the opening reserves | 23.9308284 |
| Two legs executed in sequence | 23.6965576 |

The second test sets the aggregate minimum one unit above the sequential quote. It checks error 8 and the trader, aggregator and pool balances and pair reserves. Retrying with the exact minimum succeeds. The Protocol 29 captures are from mainnet ledgers 65534382 (split) and 65534377 (minimum). Authorization is mocked locally.

[Read the Rust assertions](tests/soroswap.rs#L160).

## Reproduce locally

Use Rust 1.95.0 with the `wasm32v1-none` target. From the repo root, build the contracts, fetch the locked test dependencies once, then run the test offline.

```sh
cargo +1.95.0 build --locked --manifest-path Cargo.toml --target wasm32v1-none --profile contract-release -p aggregator-contract --target-dir integration-tests/soroswap/target
cargo +1.95.0 fetch --locked --manifest-path integration-tests/soroswap/Cargo.toml
cargo +1.95.0 test --locked --offline --manifest-path integration-tests/soroswap/Cargo.toml
```

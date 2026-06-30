# Optional mainnet fixtures

Place a base64 `TransactionEnvelope` for a real aggregator `swap` here as `swap_envelope.b64` to run the optional integration test `parses_mainnet_swap_fixture_if_present`.

Generate with:

```bash
# scan recent mainnet txs (requires network)
cargo run -p analytics-indexer --bin fetch-fixture
```

Or copy `envelopeXdr` from Soroban RPC `getTransactions` filtered by `AGGREGATOR_CONTRACT`.

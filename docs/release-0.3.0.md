# LumAgg 0.3.0

This release collects the application, API, analytics, arbitrage tooling, and
self-hosting improvements since 0.2.1.

## Highlights

- Light and dark themes, with improved contrast in light mode.
- Shareable swap URLs that preserve the selected input and output tokens.
- Balance refresh when returning to the app, refreshed quotes after completed
  transactions, and more reliable wallet loading and transaction confirmation.
- External reference prices for XLM marks and shared statistics caches across
  API replicas.
- Expanded arbitrage analytics, including execution failures and cumulative
  XLM transaction fees.
- Arbitrage quote refresh and on-chain validation before execution, improved
  scan backpressure, and clearer failure diagnostics.
- Explicit network selection in the TypeScript SDK and clearer errors for
  unsupported testnet requests to the hosted API.
- Corrected OpenAPI transaction-building response fields and reorganized
  integration and self-hosting documentation.
- Aggregator and Vault security hardening and audit-readiness documentation.

## Downloads

The release workflow builds Linux x86_64 archives and checks their contents,
binary startup, and sample configuration parsing before publication:

- `lumagg-swap-api-linux-x86_64.tar.gz`: standalone Swap API.
- `lumagg-aggregator-linux-x86_64.tar.gz`: API server, market-data worker,
  analytics indexer, configuration examples, and systemd units.
- `lumagg-arbitrage-bot-linux-x86_64.tar.gz`: arbitrage bot and operator guides.
- `lumagg-limit-keeper-linux-x86_64.tar.gz`: experimental Limit/DCA keeper.
- `SHA256SUMS`: checksums for the four archives.

## Scope and compatibility

Limit/DCA and Order Escrow remain testnet-only. Their inclusion as development
tooling does not announce a mainnet deployment.

The third-party security audit has not been completed. Automated Scout scanning
and internal hardening are not a substitute for a third-party audit. The
requested audit scope is the Aggregator and Arb Vault; Order Escrow is excluded.

The TypeScript SDK has its own version (`0.3.1` in this source tree). This GitHub
release does not publish a new npm package or deploy smart contracts.

## Hosted infrastructure validation: October 24, 2026

The hosted RPC, Horizon, and their Core processes were upgraded to Stellar
`30.0.0` and verified to resume synchronization. Checks included a live quote,
unsigned swap construction, a successful swap simulation, event queries, and
an arbitrage simulation that reached the expected economic protection check.
The arbitrage bot was resumed and the analytics indexer continued advancing.

These checks ran while Mainnet was still on Protocol 29. The node software
upgrade does not establish Protocol 30 compatibility for all application SDKs
or XDR decoders. Dependency upgrades and Protocol 30 regression testing remain
separate work before the network activation.

[Full changelog](https://github.com/Lum-Agg/stellar-dex-agg/compare/swap-api-v0.2.1...swap-api-v0.3.0)

[Documentation](https://lumagg.gitbook.io/lumagg)

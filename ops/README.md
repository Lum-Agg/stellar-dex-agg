# Maintainer Deployment Scripts

These scripts are for LumAgg maintainers to deploy the project’s own services.
The mainnet backend, indexer, and arbitrage scripts default to
`root@162.245.187.234` (override with `LUMAGG_DEPLOY_SERVER`). They require an
already configured server and retain its TOML files and systemd units.
The Limit/DCA testnet script requires an explicit `SERVER` for an isolated host.
These scripts are not the recommended self-hosting path.

For self-hosting, download the release binaries and follow:

- [Deployment Overview](../docs/deployment-overview.md)
- [Production Aggregator Deployment](../docs/aggregator-deployment.md)
- [Arbitrage Deployment](../docs/arbitrage-deployment.md)

The scripts under this directory may contain maintainer-specific host paths and
should not be copied into a third-party deployment unchanged.

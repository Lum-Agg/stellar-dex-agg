# Arbitrage Deployment

LumAgg Arbitrage is an operator-run scanner and executor. It is not part of
`lumagg-swap-api` and it should run as its own native process.

For high-throughput mainnet operation, connect it to the
[production Aggregator topology](aggregator-deployment.md). A
`lumagg-swap-api` instance can be used for evaluation or a small private setup.

## Architecture

```text
quote API -> lumagg-arbitrage-bot -> simulateTransaction -> optional transaction submit
                         -> vault -> Aggregator -> DEXes
```

The scanner requests outward and return routes, optimizes trade size, builds an
atomic round trip, and checks simulated profit after the estimated fee. Only
the final rollout stage signs and broadcasts transactions.

## Prerequisites

- One or more ready LumAgg quote API endpoints.
- A low-latency Soroban RPC with sufficient simulation and submission capacity.
- A deployed LumAgg Aggregator contract compatible with the quote API.
- Base and bridge token contract IDs selected for the scanner.
- One or more funded caller accounts for fees and transaction submission.
- Optionally, a deployed and funded LumAgg Vault with every caller allowlisted.

See [Smart Contract Deployment](contracts-deployment.md) for Aggregator and
Vault deployment, upgrades, and external TTL maintenance.

Without `contracts.vault`, caller accounts must also hold the trading
principal. With a vault, principal remains in the vault and callers normally
hold only enough native XLM for fees.

## Download

Linux x86_64 releases include a standalone arbitrage archive:

<https://github.com/Lum-Agg/stellar-dex-agg/releases>

```bash
grep 'lumagg-arbitrage-bot-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum --check
tar -xzf lumagg-arbitrage-bot-linux-x86_64.tar.gz
cd lumagg-arbitrage-bot-linux-x86_64
```

Create a private config and caller-secret file from the archive contents:

```bash
cp lumagg-arbitrage.toml arbitrage.toml
chmod 600 arbitrage.toml
touch arbitrage-callers
chmod 600 arbitrage-callers
```

Set `accounts.caller_secrets_file` in `arbitrage.toml` to the absolute path of
`arbitrage-callers`, then put one Stellar `S...` secret per line in that file.
Never commit either private file. Mnemonic-based callers are also supported
through `accounts.mnemonic_path` and `accounts.caller_indices`.

To build the same executable from a tagged source revision instead:

```bash
git clone https://github.com/Lum-Agg/stellar-dex-agg.git
cd stellar-dex-agg
git checkout <release-tag-or-commit>
cargo build --locked --release -p arbitrage --bin lumagg-arbitrage-bot
```

## Configure and run

Edit `arbitrage.toml` and set the quote URLs, RPC, contract IDs, and bridge
tokens. Start with conservative trade limits. Amounts are integer token units;
XLM uses seven decimal places.

The binary is not tied to systemd. Load the config and run it under any process
manager:

```bash
./lumagg-arbitrage-bot --config ./arbitrage.toml --check-config
./lumagg-arbitrage-bot --config ./arbitrage.toml
```

The remaining rollout stages apply regardless of the process manager.

## Optional systemd example

The release archive also includes a systemd unit. Install it only if systemd is
your chosen process manager:

```bash
sudo useradd --system --home /var/lib/lumagg --shell /usr/sbin/nologin lumagg
sudo install -d -o lumagg -g lumagg -m 0750 /var/lib/lumagg
sudo install -d -o root -g lumagg -m 0750 /etc/lumagg
sudo install -m 0755 lumagg-arbitrage-bot /usr/local/bin/
sudo install -m 0600 -o lumagg -g lumagg /dev/null \
  /etc/lumagg/arbitrage-callers
sudo install -m 0640 -o root -g lumagg arbitrage.toml /etc/lumagg/arbitrage.toml
sudo install -m 0644 lumagg-arbitrage.service \
  /etc/systemd/system/
```

Skip `useradd` when the service account already exists. Set
`accounts.caller_secrets_file = "/etc/lumagg/arbitrage-callers"` in the installed config.

## Safe Rollout

Do not move directly from installation to live submission. Use these stages.

### 1. Quote-only scan

```toml
[execution]
build_tx = false
submit_tx = false
dry_run = true
```

This validates quote connectivity, route discovery, concurrency, and sizing
without building or simulating transactions.

### 2. Build and simulate

```toml
[execution]
build_tx = true
submit_tx = false
dry_run = true
```

This requires valid contracts and caller accounts. Confirm simulation success,
authorization, fees, vault balance, and post-fee profit behavior before
continuing.

### 3. Live submission

```toml
[execution]
build_tx = true
submit_tx = true
dry_run = false
```

Enable this only after reviewing sustained simulation logs and funding limits.
Restart the service after each configuration change:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now lumagg-arbitrage
journalctl -u lumagg-arbitrage -f
```

## Profit and Failure Policy

Before submission, the scanner gates an opportunity using the simulated return
minus the estimated transaction fee and the configured min profit
(`scanner.min_profit`, or the per-base `scanner.min_profit_xlm` /
`scanner.min_profit_usdc`).
The transaction's on-chain `min_amount_out` is intentionally only greater than
the input amount, instead of encoding the entire simulated profit target. This
reduces avoidable post-submission failures and fee loss when execution changes
slightly, while still requiring a positive round-trip token return.

This policy does not guarantee fiat profit and does not remove latency,
liquidity, RPC, contract, or fee risk. The operator controls account funding,
vault exposure, token selection, and submission settings.

## Operations

- Keep the quote API and RPC close to the scanner; latency directly affects
  opportunity validity.
- Keep `scanner.max_splits = 1` initially. More splits increase route complexity and
  Soroban resource use.
- Use several callers only after confirming sequence management and fee
  funding for one caller.
- Monitor simulation failure rate, estimated fees, successful transactions,
  realized token deltas, RPC errors, and vault balance independently.
- To stop new submissions immediately, set `execution.submit_tx = false` and restart, or
  stop `lumagg-arbitrage.service`.

The deeper contract and fund-flow description remains in
[Round-trip Arbitrage](round-trip-arb.md).

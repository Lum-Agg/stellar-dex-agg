# Arbitrage Configuration Reference

`lumagg-arbitrage-bot` reads a TOML file and applies it to the scanner runtime.
The release archive includes a complete `lumagg-arbitrage.toml` template.
Unknown sections and unknown keys are rejected to catch spelling mistakes.

```bash
./lumagg-arbitrage-bot --config ./arbitrage.toml --check-config
```

The config can reference caller secrets, mnemonic files, private RPC URLs, and
Telegram credentials. Store it outside the repository and restrict it to the
service account:

```bash
chmod 600 arbitrage.toml
chmod 600 arbitrage-callers
```

## Network

| TOML key | Required/default | Description |
| --- | --- | --- |
| `network.rpc_url` | required | Soroban RPC used for simulation, transaction preparation, submission, and account sequence reads. Use a low-latency endpoint with enough capacity. |
| `network.quote_api_urls` | required | One or more LumAgg quote API base URLs. The bot can distribute quote work across multiple API replicas. |

## Contracts

| TOML key | Required/default | Description |
| --- | --- | --- |
| `contracts.aggregator` | required | LumAgg Aggregator contract used by built round-trip transactions. Must match the quote API's contract version. |
| `contracts.vault` | unset | Optional LumAgg Vault. When set, callers execute through the vault and principal stays in the vault. When unset, callers must hold the trading principal directly. |

## Accounts

| TOML key | Required/default | Description |
| --- | --- | --- |
| `accounts.caller_secrets_file` | unset | Path to a file containing one Stellar `S...` secret seed per line. |
| `accounts.mnemonic_path` | unset | Path to a mnemonic file used with `caller_indices`. |
| `accounts.caller_indices` | `[0]` | Derivation indices used when `mnemonic_path` is configured. |

Use either direct caller secrets or a mnemonic. Caller accounts must have enough
XLM for fees. When `contracts.vault` is configured, the vault holds the trading
principal and callers do not need classic trustlines for the intermediate or
base assets. Without a vault, callers must hold the trading principal directly,
including any required balances and trustlines.

This does not repair an invalid DEX pool. A Soroban pool account that lacks the
trustline for one of its classic-backed assets can still make simulation fail;
remove or repair that pool in the venue data before enabling the route.

## Assets

| TOML key | Required/default | Description |
| --- | --- | --- |
| `assets.base_tokens` | XLM and USDC when omitted | Tokens used as the starting and ending asset for round trips. |
| `assets.bridge_tokens` | required | Intermediate tokens scanned between each base token pair, for example XLM -> bridge -> XLM. |

The scanner creates base-token and bridge-token combinations from these lists.
Keep the first production set small, then expand after observing quote latency,
simulation success rate, and API backlog.

## Scanner

| TOML key | Program default | Description |
| --- | --- | --- |
| `scanner.probe_amount_in` | runtime default | Initial amount, in raw token units, used to probe opportunities. XLM and most Stellar SAC assets use 7 decimals. |
| `scanner.min_profit` | runtime default | Global minimum simulated post-fee profit in raw base-token units. |
| `scanner.min_profit_xlm` | unset | XLM-specific minimum profit override. |
| `scanner.min_profit_usdc` | unset | USDC-specific minimum profit override. |
| `scanner.xlm_usdc_price_e7` | runtime default | XLM/USDC price scaled by 1e7 for economics and alerts. |
| `scanner.xlm_usdc_price_refresh_secs` | runtime default | Interval for refreshing the XLM/USDC reference price. |
| `scanner.slippage_bps` | runtime default | Quote-side slippage parameter. The final on-chain round-trip still uses a positive-return threshold. |
| `scanner.max_hops` | runtime default | Maximum hops per outward or return route. |
| `scanner.max_splits` | runtime default | Maximum route splits per quote. Start with `1` for production arbitrage. |
| `scanner.on_chain_validate` | runtime default | Enables quote-side on-chain validation before build/simulation. |
| `scanner.scan_interval_ms` | runtime default | Delay between scan cycles. |
| `scanner.item_gap_ms` | runtime default | Delay between queued base/bridge scan items. |
| `scanner.worker_count` | runtime default | Number of opportunity workers. Tune together with quote API and RPC capacity. |
| `scanner.optimize_amount` | runtime default | Enables amount optimization instead of only probing one size. |
| `scanner.min_amount_in` | unset | Lower bound for optimized trade size. |
| `scanner.max_amount_in` | runtime default | Upper bound for optimized trade size. |
| `scanner.sample_count` | runtime default | Number of samples used during amount optimization. |

## Execution

| TOML key | Program default | Description |
| --- | --- | --- |
| `execution.build_tx` | `false` in template | Build and simulate candidate round-trip transactions. |
| `execution.submit_tx` | `false` in template | Submit signed transactions after simulation passes. |
| `execution.dry_run` | `true` in template | Prevent live execution behavior during quote-only or simulation-only testing. |
| `execution.poll_tx` | runtime default | Poll submitted transaction status. |
| `execution.submit_dedup_secs` | runtime default | Time window used to avoid submitting duplicate opportunities. |
| `execution.caller_cooldown_ms` | runtime default | Cooldown after a caller is used, protecting sequence-number safety. |

Use the rollout stages in [Arbitrage Deployment](arbitrage-deployment.md):
quote-only, build-and-simulate, then live submission.

## Monitoring

| TOML key | Program default | Description |
| --- | --- | --- |
| `monitoring.log_filter` | `info` | Rust tracing filter. |
| `monitoring.telegram_enabled` | `false` | Enables Telegram alerts. |
| `monitoring.telegram_bot_token` | unset | Telegram bot token. Treat it as a secret. |
| `monitoring.telegram_chat_id` | unset | Destination chat ID. |
| `monitoring.telegram_interval_secs` | runtime default | Periodic summary interval. |
| `monitoring.quiet_alert_tick_secs` | runtime default | Interval for evaluating quiet-window alerts. |
| `monitoring.quiet_alert_cooldown_secs` | runtime default | Minimum spacing between quiet-window alerts. |
| `monitoring.quiet_alert_windows` | runtime default | Number of quiet windows considered before alerting. |
| `monitoring.quiet_alert_min_opportunities` | runtime default | Minimum expected opportunities before quiet-window alerts matter. |

## Economics note

Before submission, the bot gates candidates using simulated return minus the
estimated fee and configured minimum profit. The transaction's on-chain
`min_amount_out` is intentionally looser: it only requires a positive base-token
return over the input. This reduces avoidable submitted-transaction failures
when execution changes slightly after simulation, but it means operators must
monitor realized surplus and fees outside the contract event itself.

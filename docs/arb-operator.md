# LumAgg atomic arbitrage — operator playbook

Tranche 2 deliverable: self-deploy **aggregator + vault + arb scanner** on mainnet.

**Not for retail users.** The vault pools trading capital; bot callers only need XLM for Soroban fees.

## Architecture

```text
quote-api (×N) ──► lumagg-arbitrage-bot ──► vault.execute_round_trip ──► aggregator.round_trip_swap ──► DEX pools
                      ▲                    │
                 Redis pool state          └── principal + profit back to vault
```


|     |     |
| --- | --- |
|     |     |
|     |     |
|     |     |
|     | ·   |


Contract details: [contracts/vault/README.md](../contracts/vault/README.md).

## Prerequisites

- Mainnet Soroban RPC (local node or gateway)
- Redis with pool-state keys (same as production quote API)
- Admin wallet: deploy vault, `deposit`, `add_caller`
- Mnemonic or secrets for N caller accounts (hot wallets, fee-only)



## 1. Deploy vault (one-time)

```bash
cd contracts/vault
ADMIN=admin ADMIN_G=G... CALLER=G... ./deploy.sh
# Record VAULT contract id → ARB_VAULT_CONTRACT
```

Fund vault:

```bash
# deposit XLM (SAC) into vault from ops wallet
stellar contract invoke --id $VAULT -- deposit --from OPS_G --token $XLM_SAC --amount <stroops>
```

Allowlist each bot caller:

```bash
stellar contract invoke --id $VAULT -- add_caller --caller GCALLER...
```



## 2. Configure arb bot

Copy [scripts/arb.env.example](../scripts/arb.env.example) → `deploy/arb.env` on server.


| Variable                                   | Purpose                                      |
| ------------------------------------------ | -------------------------------------------- |
| `ARB_VAULT_CONTRACT`                       | Vault contract id                            |
| `ARB_AGGREGATOR_CONTRACT`                  | LumAgg aggregator                            |
| `ARB_MNEMONIC_PATH` + `ARB_CALLER_INDICES` | HD-derived callers (e.g. `1,2,…,9`)          |
| `ARB_QUOTE_API_URLS`                       | Round-robin quote APIs (`3100–3103`)         |
| `ARB_BRIDGE_TOKENS`                        | SAC hubs for round-trip bridge legs          |
| `ARB_BASE_TOKENS`                          | Usually XLM SAC                              |
| `ARB_SUBMIT_TX`                            | `0` = build/simulate only; `1` = live submit |
| `ARB_DRY_RUN`                              | `1` = log opportunities without chain tx     |
| `ARB_MIN_PROFIT`                           | Default post-fee net floor (base units, 7dp) |
| `ARB_MIN_PROFIT_XLM` / `ARB_MIN_PROFIT_USDC` | Optional per-base floors                     |
| `ARB_XLM_USDC_PRICE_E7` | Fallback USDC units per 1.0 XLM when live quote fails (default `1800000`) |
| `ARB_XLM_USDC_PRICE_REFRESH_SECS` | Refresh live XLM→USDC mark from quote-api (default `60`; `0` = fallback only) |
| `ARB_MAX_AMOUNT_IN`                        | Soft ceiling; also capped by vault base SAC balance when vault is set |
| `ARB_OPTIMIZE_AMOUNT`                      | `1` (default) = always log-space size search (even if probe is flat) |
| `ARB_ON_CHAIN_VALIDATE`                    | `0` (default) = off. `1` = quote-api hop validate (slow; diagnostics only) |
| `ARB_TELEGRAM_INTERVAL_SECS`               | Hourly P&L report (optional)                 |


Deploy:

```bash
./deploy_arb.sh
systemctl status lumagg-arb
```



## 3. Safe rollout

1. `ARB_DRY_RUN=1`**,** `ARB_SUBMIT_TX=0` — confirm quotes and route labels in logs.
2. `ARB_SUBMIT_TX=0`**, build+simulate** — verify `min_amount_out` and fee estimates.
3. `ARB_SUBMIT_TX=1` with one caller — watch vault balance and first SUCCESS hashes.
4. Scale callers — vault balance ≥ `concurrent_txs × amount_in`.



## 4. Monitoring


| Signal                             | Where                                        |
| ---------------------------------- | -------------------------------------------- |
| SUCCESS / FAILED txs               | `journalctl -u lumagg-arb`                   |
| Hourly gross/fees/net + funnel     | Telegram (`deploy/telegram.env`)             |
| Quiet window (opps but 0 prepares) | Telegram alert `arb_quiet_window`            |
| On-chain volume                    | `lumagg-analytics-indexer --config aggregator.toml status` / `/api/v1/stats` |
| Caller XLM                         | Horizon account balance (fee float only)     |




### Quote → sim funnel (long-term)

Local quotes can look profitable while Soroban simulation loses ~20 bps. Prefer fixing the **quote path** (fresher pool state / venue math / optional on-chain hop validation) over execution-side workarounds. Scanner already size-optimizes even when the 10 XLM probe is flat; post-fee `ARB_MIN_PROFIT` still applies after simulate.

Arb requests quote-api with `on_chain_validate=1` only when `ARB_ON_CHAIN_VALIDATE=1` (default **off** — hop RPCs slow the scan). Retail UI leaves it off. Manual check:

```bash
curl -sG https://api.lumagg.xyz/api/v1/quote \
  --data-urlencode "token_in=$XLM" \
  --data-urlencode "token_out=$USDC" \
  --data-urlencode "amount_in=100000000" \
  --data-urlencode "prefer_soroban=1" \
  --data-urlencode "on_chain_validate=1" | jq '.data | {expected_output, on_chain_validated}'
```

Every 5 minutes `arb stats summary` logs:

The plain fields are cumulative since the bot process started. The
`delta_*` fields report only the new activity since the previous five-minute
report, so they can be summed for daily monitoring without restarting the bot.


| Field                       | Meaning                                                       |
| --------------------------- | ------------------------------------------------------------- |
| `prepare_rate_bps`          | `prepared / opportunities`                                    |
| `sim_reject_rate_bps`       | `sim_profit_rejected / opportunities`                         |
| `discard_size_unprofitable` | Optimized size failed break-even on-chain                     |
| `discard_below_quoted`      | Route ran on-chain but below quoted profit                    |
| `discard_fee_gate`          | Sim OK but net after fees < `ARB_MIN_PROFIT`                  |
| `avg_quote_sim_gap_bps`     | Mean (quoted_bps − on_chain_bps); positive = quote optimistic |
| `delta_opportunities`       | New opportunities since the previous report                    |
| `delta_prepared`            | New prepared transactions since the previous report             |
| `delta_sim_profit_rejected` | New economic simulation rejections since the previous report    |
| `delta_submitted`           | New submitted transactions since the previous report             |
| `delta_succeeded`           | New successful transactions since the previous report            |
| `delta_failed`              | New failed transactions since the previous report                |


Quiet-window Telegram alert (default: 5×60s ticks with ≥50 new opportunities and 0 prepares):


| Env                             | Default | Purpose                              |
| ------------------------------- | ------- | ------------------------------------ |
| `ARB_QUIET_ALERT_TICK_SECS`     | `60`    | Tracker tick                         |
| `ARB_QUIET_ALERT_WINDOWS`       | `5`     | Consecutive quiet ticks before alert |
| `ARB_QUIET_ALERT_MIN_OPPS`      | `50`    | Min Δopportunities per tick          |
| `ARB_QUIET_ALERT_COOLDOWN_SECS` | `1800`  | Telegram rate limit                  |


Example log grep:

```bash
journalctl -u lumagg-arb --since today | grep -E 'arb tx SUCCESS|arb stats summary|quiet window'
```



### Quote vs on-chain probe (offline)

Does **not** run inside `lumagg-arb`. Samples quote-api routes and immediately
compares each hop to on-chain `estimate_swap` / fresh Soroswap reserves (and
optionally a full vault `simulateTransaction`).

Round-trip mode needs `ARB_BRIDGE_TOKENS` or `--bridges C...,C...`. With
`--simulate`, set `ARB_VAULT_CONTRACT` and `ARB_AGGREGATOR_CONTRACT` (typically
`source deploy/arb.env`).

```bash
./scripts/quote-sim-probe.sh --mode round-trip --samples 20 --seed 1 \
  --amount-in 100000000 --jsonl --simulate
```

Standalone without full `arb.env`:

```bash
./scripts/quote-sim-probe.sh --mode round-trip --bridges C...,C... --samples 5
```

Use when quotes look optimistic vs on-chain, or when prepares go quiet despite
opportunities (those signals may appear in arb stats / Telegram). Path-level
`gap_bps` is authoritative; hop `chain_out` shows where the chain path shrinks.

**Scheduled (production):** systemd timer every **30 minutes** (not a resident
process). Installed by `./deploy_arb.sh`:

```bash
systemctl status lumagg-quote-sim-probe.timer
journalctl -u lumagg-quote-sim-probe -n 50 --no-pager
# JSONL archive:
tail -f /opt/stellar-dex-aggregator/logs/quote-sim-probe.jsonl
```

Defaults: 10 samples, `--simulate`, threshold 30 bps (exit 1 if median gap
exceeds it). Override via `deploy/arb.env` (`PROBE_SAMPLES`,
`PROBE_THRESHOLD_BPS`, `PROBE_SIMULATE=0`). For hourly instead of 30m, edit
`deploy/lumagg-quote-sim-probe.timer` → `OnCalendar=hourly` and
`systemctl daemon-reload && systemctl restart lumagg-quote-sim-probe.timer`.

Manual oneshot: `systemctl start lumagg-quote-sim-probe.service`

## 5. Risk & limits

- **Arb-only vault** — no public withdraw; callers cannot drain in a separate tx.
- **Multi-hop fees** — Soroban resource fees scale with legs; gate on **net** profit (`ARB_MIN_PROFIT`).
- **Caller rotation** — round-robin acquire avoids single-caller starvation.
- **Trustlines** — arb uses SAC; callers should **not** need classic trustlines (clean with `clean_caller_trustlines` if legacy cron ran).
- **Contract upgrades** — WASM upload costs ~20 XLM on mainnet; coordinate before upgrade.



## 6. Operator checklist

| Item | Where to verify |
| --- | --- |
| Mainnet vault id | `contracts/vault/README.md` + deploy tx |
| Dry-run / simulated txs | `journalctl` for the arb service |
| On-chain round-trip | tx hash in logs + [analytics indexer](./analytics-indexer.md) |
| This runbook | keep env and risk limits current |

## 7. Related docs

- [integrator-guide.md](./integrator-guide.md) — quote API for integrators
- [analytics-indexer.md](./analytics-indexer.md) — on-chain stats pipeline
- [round-trip-arb.md](./round-trip-arb.md) — vault + `round_trip_swap` overview

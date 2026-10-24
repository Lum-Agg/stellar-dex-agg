//! Hourly Telegram profit / balance report + quiet-window alerts for
//! arb-scanner.

use {
    crate::{
        economics::{USDC_SAC, XLM_SAC},
        prepare::fetch_account_native_balance,
        profit::{format_xlm4, format_xlm4_u, ProfitWindow, RecentTx},
        runtime::ArbRuntime,
        stats::{BridgeStatsSnapshot, QuietWindowTracker},
        vault::fetch_token_balance_stroops,
    },
    lumagg_alerts::TelegramAlerter,
    std::sync::Arc,
    tracing::{info, warn},
};

pub fn spawn_hourly_profit_report(runtime: Arc<ArbRuntime>, alerter: Arc<TelegramAlerter>) {
    let interval_secs = std::env::var("ARB_TELEGRAM_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600u64)
        .max(60);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Skip immediate first tick — wait a full interval after startup.
        interval.tick().await;

        loop {
            interval.tick().await;
            match build_profit_report(&runtime).await {
                Ok(msg) => {
                    if let Err(e) = send_report(&alerter, &msg).await {
                        warn!(error = %e, "arb telegram profit report failed");
                    } else {
                        info!("arb telegram profit report sent");
                    }
                }
                Err(e) => warn!(error = %e, "arb telegram profit report build failed"),
            }
        }
    });
}

async fn send_report(alerter: &TelegramAlerter, text: &str) -> anyhow::Result<()> {
    for part in report_parts(text) {
        alerter.send(&part).await?;
    }
    Ok(())
}

/// Leave headroom below the message limit; preserve lines where possible and
/// count UTF-16 units so non-BMP characters cannot exceed the per-part budget.
fn report_parts(text: &str) -> Vec<String> {
    const BUDGET: usize = 3500;
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut units = 0;
    for line in text.split_inclusive('\n') {
        if !part.is_empty() && units + line.encode_utf16().count() > BUDGET {
            parts.push(std::mem::take(&mut part));
            units = 0;
        }
        for ch in line.chars() {
            if units + ch.len_utf16() > BUDGET {
                parts.push(std::mem::take(&mut part));
                units = 0;
            }
            part.push(ch);
            units += ch.len_utf16();
        }
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// Alert when opportunities keep arriving but nothing prepares (quote/sim gap).
pub fn spawn_quiet_window_monitor(runtime: Arc<ArbRuntime>, alerter: Arc<TelegramAlerter>) {
    let tick_secs = std::env::var("ARB_QUIET_ALERT_TICK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64)
        .max(15);
    let cooldown_secs = std::env::var("ARB_QUIET_ALERT_COOLDOWN_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800u64)
        .max(60);

    tokio::spawn(async move {
        let mut tracker = QuietWindowTracker::from_env();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(tick_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            interval.tick().await;
            let snap = runtime.stats.snapshot();
            let Some(alert) = tracker.observe(snap) else {
                continue;
            };
            info!(
                consecutive_windows = alert.consecutive_windows,
                opportunities_delta = alert.opportunities_delta,
                avg_quote_sim_gap_bps = alert.avg_quote_sim_gap_bps,
                "arb quiet window detected"
            );
            let text = alert.telegram_text();
            if let Err(e) = alerter
                .send_rate_limited("arb_quiet_window", &text, std::time::Duration::from_secs(cooldown_secs))
                .await
            {
                warn!(error = %e, "arb quiet-window telegram alert failed");
            }
        }
    });
}

async fn build_profit_report(runtime: &ArbRuntime) -> anyhow::Result<String> {
    let (hour, session, recent) = runtime.profit.snapshot_for_hourly_report();
    let vault_balances = fetch_vault_balances(runtime).await;
    let caller_lines = fetch_caller_balances(runtime).await;
    let funnel = runtime.stats.snapshot();
    let bridge_breakdown = runtime.stats.bridge_breakdown();
    let sim_failure_breakdown = runtime.stats.sim_failure_breakdown(4);
    let mut report = format_report(
        &vault_balances,
        &caller_lines,
        &hour,
        &session,
        &recent,
        &funnel,
        &bridge_breakdown,
        &sim_failure_breakdown,
    );
    if let Some(ledger) = &runtime.submission_ledger {
        match ledger.unresolved_counts() {
            Ok((pending, history)) => report.push_str(&format!(
                "\n\nReconciliation: {pending} unresolved ({history} need historical verification)"
            )),
            Err(error) => {
                warn!(%error, "submission ledger counts unavailable");
                report.push_str("\n\nReconciliation: unavailable (database error)");
            }
        }
    }
    let dropped = runtime
        .stats
        .simulations
        .dropped_dimensions
        .load(std::sync::atomic::Ordering::Relaxed);
    if dropped > 0 {
        report.push_str(&format!(
            "\nDiagnostics: {dropped} dimension observations dropped (session capacity limit)"
        ));
    }
    Ok(report)
}

async fn fetch_vault_balances(runtime: &ArbRuntime) -> Vec<(String, Option<u128>)> {
    let Some(vault) = runtime.config.vault_contract.as_deref() else {
        return Vec::new();
    };
    let mut balances = Vec::with_capacity(runtime.config.base_tokens.len());
    for base in &runtime.config.base_tokens {
        let token = base.canonical();
        let balance = fetch_token_balance_stroops(&runtime.config.rpc_url, &token, vault)
            .await
            .ok();
        balances.push((token, balance));
    }
    balances
}

async fn fetch_caller_balances(runtime: &ArbRuntime) -> Vec<(usize, String, Option<u128>)> {
    let Some(pool) = &runtime.caller_pool else {
        return Vec::new();
    };
    let keys = pool.public_keys();
    let mut out = Vec::with_capacity(keys.len());
    for (i, pk) in keys.into_iter().enumerate() {
        let bal = fetch_account_native_balance(&runtime.config.rpc_url, &pk).await.ok();
        out.push((i + 1, pk, bal));
    }
    out
}

fn format_window(label: &str, base: &str, w: &ProfitWindow) -> String {
    let net = if w.incomplete > 0 {
        "n/a (incomplete accounting)".to_owned()
    } else {
        format_xlm4(w.net_profit_stroops())
    };
    format!(
        "{label}\n\
         · succeeded: {}\n\
         · failed: {}\n\
         · poll timeouts (observed): {}\n\
         · submitted: {}\n\
         · actual gross profit: `{}` {base}\n\
         · actual fees (success + failed): `{}` XLM\n\
         · fee value at submission mark: `{}` {base}\n\
         · net (marked for non-XLM): `{}` {base}\n\
         · incomplete accounting: {}\n\
         · simulated gross / est. resource fees: `{}` {base} / `{}` XLM",
        w.succeeded,
        w.failed,
        w.unknown,
        w.submitted,
        format_xlm4(w.gross_profit_stroops),
        format_xlm4_u(w.actual_fee_xlm_stroops),
        format_xlm4_u(w.fee_stroops),
        net,
        w.incomplete,
        format_xlm4(w.estimated_gross),
        format_xlm4_u(w.estimated_fee_xlm_stroops),
    )
}

fn format_recent(recent: &[RecentTx]) -> String {
    if recent.is_empty() {
        return "Recent SUCCESS: (none)".to_string();
    }
    let mut lines = vec!["Recent SUCCESS (net marked for non-XLM):".to_string()];
    for tx in recent {
        let net = tx.gross_profit.saturating_sub(tx.fee.min(i128::MAX as u128) as i128);
        lines.push(format!(
            "· {} {} → net `{}` | `{}`",
            format_xlm4_u(tx.amount_in),
            base_label(&tx.base_token),
            format_xlm4(net),
            &tx.hash[..tx.hash.len().min(12)],
        ));
    }
    lines.join("\n")
}

fn format_bridge_breakdown(rows: &[BridgeStatsSnapshot]) -> String {
    let mut ranked: Vec<_> = rows.iter().filter(|row| row.evaluated > 0).collect();
    ranked.sort_by(|a, b| {
        b.opportunities
            .cmp(&a.opportunities)
            .then_with(|| b.evaluated.cmp(&a.evaluated))
            .then_with(|| a.bridge.cmp(&b.bridge))
    });

    if ranked.is_empty() {
        return "🧭 Bridge funnel: (no scans yet)".to_string();
    }

    let mut lines = vec!["🧭 Bridge funnel (top 6):".to_string()];
    for row in ranked.into_iter().take(6) {
        let quote_fail_bps = row.quote_failed.saturating_mul(10_000) / row.evaluated.max(1);
        lines.push(format!(
            "· {}: eval={} opp={} unprof={} quote_fail={} ({} bps)",
            row.bridge, row.evaluated, row.opportunities, row.unprofitable_quotes, row.quote_failed, quote_fail_bps,
        ));
    }
    lines.join("\n")
}

pub fn format_report(
    vault_balances: &[(String, Option<u128>)],
    callers: &[(usize, String, Option<u128>)],
    hour: &std::collections::BTreeMap<String, ProfitWindow>,
    session: &std::collections::BTreeMap<String, ProfitWindow>,
    recent: &[RecentTx],
    funnel: &crate::stats::ArbStatsSnapshot,
    bridge_breakdown: &[BridgeStatsSnapshot],
    sim_failure_breakdown: &[(String, u64)],
) -> String {
    let vault_lines = if vault_balances.is_empty() {
        "🏦 Vault balances: `(n/a)`".to_string()
    } else {
        vault_balances
            .iter()
            .map(|(token, balance)| {
                let label = base_label(token);
                match balance {
                    Some(v) => format!("🏦 Vault {label}: `{}` {label}", format_xlm4_u(*v)),
                    None => format!("🏦 Vault {label}: `(n/a)`"),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut caller_block = String::from("👛 Caller Accounts:\n");
    let mut caller_total = 0u128;
    for (idx, _pk, bal) in callers {
        match bal {
            Some(b) => {
                caller_total = caller_total.saturating_add(*b);
                caller_block.push_str(&format!("idx={idx}: `{}` XLM\n", format_xlm4_u(*b)));
            }
            None => caller_block.push_str(&format!("idx={idx}: `(err)`\n")),
        }
    }
    caller_block.push_str(&format!("💰 Total Caller XLM: `{}` XLM", format_xlm4_u(caller_total)));

    let grand_xlm = vault_balances
        .iter()
        .find(|(token, _)| base_label(token) == "XLM")
        .and_then(|(_, balance)| *balance)
        .unwrap_or(0)
        .saturating_add(caller_total);

    let funnel_block = format!(
        "🔎 Quote→sim funnel (session):\n\
         · quote_failed: {}\n\
         · unprofitable: {}\n\
         · opportunities: {}\n\
         · caller_busy: {}\n\
         · prepared: {} ({} bps)\n\
         · sim_profit_rejected: {} ({} bps)\n\
         · discards: size={} below_quoted={} fee={} probe={}\n\
         · avg quote−sim gap: `{}` bps (n={})",
        funnel.quote_failed,
        funnel.unprofitable_quotes,
        funnel.opportunities,
        funnel.caller_busy,
        funnel.txs_prepared,
        funnel.prepare_rate_bps(),
        funnel.txs_sim_profit_rejected,
        funnel.sim_reject_rate_bps(),
        funnel.discard_size_unprofitable,
        funnel.discard_below_quoted,
        funnel.discard_fee_gate,
        funnel.discard_probe_unprofitable,
        funnel.avg_quote_sim_gap_bps,
        funnel.quote_sim_gap_samples,
    );

    let failure_block = if sim_failure_breakdown.is_empty() {
        "🧪 Sim failure reasons: (none)".to_string()
    } else {
        let rows = sim_failure_breakdown
            .iter()
            .map(|(reason, count)| format!("{reason}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("🧪 Sim failure reasons: {rows}")
    };

    format!(
        "📊 LumAgg Arb Monitor\n\
         \n\
         {vault_lines}\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {funnel_block}\n\
         \n\
         {failure_block}\n\
         \n\
         {}\n\
         \n\
         {caller_block}\n\
         ✅ Grand Total XLM: `{}` XLM",
        format_windows("⏱ Last hour:", hour),
        format_windows("📈 Session:", session),
        format_recent(recent),
        format_bridge_breakdown(bridge_breakdown),
        format_xlm4_u(grand_xlm),
    )
}

fn format_windows(label: &str, windows: &std::collections::BTreeMap<String, ProfitWindow>) -> String {
    if windows.is_empty() {
        return format!("{label}\n· (no trades)");
    }
    windows
        .iter()
        .map(|(base, window)| {
            let display_base = base_label(base);
            format_window(&format!("{label} {display_base}"), display_base, window)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn base_label(base: &str) -> &str {
    if base == XLM_SAC || base == "XLM" {
        "XLM"
    } else if base == USDC_SAC || base == "USDC" {
        "USDC"
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{profit::ProfitWindow, stats::ArbStatsSnapshot},
    };

    fn empty_funnel() -> ArbStatsSnapshot {
        ArbStatsSnapshot {
            routes_evaluated: 1000,
            quote_failed: 0,
            unprofitable_quotes: 99,
            opportunities: 100,
            txs_prepared: 1,
            txs_sim_rejected: 0,
            txs_sim_profit_rejected: 90,
            discard_size_unprofitable: 50,
            discard_below_quoted: 30,
            discard_fee_gate: 10,
            discard_probe_unprofitable: 0,
            avg_quote_sim_gap_bps: 20,
            quote_sim_gap_samples: 80,
            txs_dry_run: 0,
            txs_submitted: 1,
            txs_succeeded: 1,
            txs_failed: 0,
            txs_dedup_skipped: 0,
            caller_busy: 0,
        }
    }

    #[test]
    fn report_parts_preserve_lines_and_all_content() {
        let line = format!("{}\n", "x".repeat(79));
        let message = line.repeat(200);
        let parts = report_parts(&message);
        assert!(parts.len() > 1);
        assert_eq!(parts.concat(), message);
        assert!(parts
            .iter()
            .all(|part| part.ends_with('\n') && part.encode_utf16().count() <= 3500));
        assert_eq!(report_parts("short"), vec!["short"]);
        assert!(report_parts("").is_empty());
    }

    #[test]
    fn report_parts_handle_long_lines_and_non_bmp_text() {
        for message in ["x".repeat(9000), "\u{1f680}".repeat(5000)] {
            let parts = report_parts(&message);
            assert_eq!(parts.concat(), message);
            assert!(parts
                .iter()
                .all(|part| !part.is_empty() && part.encode_utf16().count() <= 3500));
        }
    }

    #[test]
    fn incomplete_accounting_has_no_numeric_net() {
        let window = ProfitWindow {
            incomplete: 1,
            succeeded: 1,
            actual_fee_xlm_stroops: 100_000,
            ..Default::default()
        };
        let text = format_window("Last hour", "USDC", &window);
        assert!(text.contains("n/a (incomplete accounting)"));
        assert!(text.contains("actual fees (success + failed): `0.0100` XLM"));
        assert!(!text.contains("est. fees:"));
    }

    #[test]
    fn report_contains_sections() {
        let hour = std::collections::BTreeMap::from([
            (
                String::from("XLM"),
                ProfitWindow {
                    succeeded: 2,
                    failed: 1,
                    unknown: 0,
                    submitted: 3,
                    gross_profit_stroops: 500_000,
                    fee_stroops: 1_000_000,
                    ..Default::default()
                },
            ),
            (
                String::from("USDC"),
                ProfitWindow {
                    succeeded: 1,
                    failed: 0,
                    unknown: 0,
                    submitted: 1,
                    gross_profit_stroops: 92_342,
                    fee_stroops: 34_547,
                    ..Default::default()
                },
            ),
        ]);
        let session = hour.clone();
        let recent = vec![RecentTx {
            hash: "57132675d4897067".into(),
            base_token: "XLM".into(),
            amount_in: 100_000_000,
            gross_profit: 380_000,
            fee: 1_074_562,
        }];
        let msg = format_report(
            &[
                (String::from("XLM"), Some(18_000_000_000)),
                (String::from("USDC"), Some(2_500_000_000)),
            ],
            &[(1, "GAAA".into(), Some(595_035_000))],
            &hour,
            &session,
            &recent,
            &empty_funnel(),
            &[],
            &[],
        );
        assert!(msg.contains("LumAgg Arb Monitor"));
        assert!(msg.contains("Vault USDC: `250.0000` USDC"));
        assert!(msg.contains("Last hour"));
        assert!(msg.contains("Last hour: USDC"));
        assert!(msg.contains("gross profit: `0.0092` USDC"));
        assert!(msg.contains("57132675d489"));
        assert!(msg.contains("59.5035"));
        assert!(msg.contains("Quote→sim funnel"));
        assert!(msg.contains("avg quote−sim gap"));
        assert!(msg.contains("below_quoted=30"));
    }
}

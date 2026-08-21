//! Hourly Telegram profit / balance report + quiet-window alerts for
//! arb-scanner.

use {
    crate::{
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
                    if let Err(e) = alerter.send(&msg).await {
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
    let vault_xlm = fetch_vault_xlm(runtime).await;
    let caller_lines = fetch_caller_balances(runtime).await;
    let funnel = runtime.stats.snapshot();
    let bridge_breakdown = runtime.stats.bridge_breakdown();
    Ok(format_report(
        vault_xlm,
        &caller_lines,
        &hour,
        &session,
        &recent,
        &funnel,
        &bridge_breakdown,
    ))
}

async fn fetch_vault_xlm(runtime: &ArbRuntime) -> Option<u128> {
    let vault = runtime.config.vault_contract.as_deref()?;
    let base = runtime.config.base_tokens.first()?;
    fetch_token_balance_stroops(&runtime.config.rpc_url, &base.canonical(), vault)
        .await
        .ok()
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

fn format_window(label: &str, w: &ProfitWindow) -> String {
    format!(
        "{label}\n\
         · succeeded: {}\n\
         · failed: {}\n\
         · unknown: {}\n\
         · submitted: {}\n\
         · gross profit: `{}` XLM\n\
         · est. fees: `{}` XLM\n\
         · net: `{}` XLM",
        w.succeeded,
        w.failed,
        w.unknown,
        w.submitted,
        format_xlm4_u(w.gross_profit_stroops),
        format_xlm4_u(w.fee_stroops),
        format_xlm4(w.net_profit_stroops()),
    )
}

fn format_recent(recent: &[RecentTx]) -> String {
    if recent.is_empty() {
        return "Recent SUCCESS: (none)".to_string();
    }
    let mut lines = vec!["Recent SUCCESS:".to_string()];
    for tx in recent {
        let net = tx.gross_profit as i128 - tx.fee as i128;
        lines.push(format!(
            "· {} XLM → net `{}` | `{}`",
            format_xlm4_u(tx.amount_in),
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
    vault_xlm: Option<u128>,
    callers: &[(usize, String, Option<u128>)],
    hour: &ProfitWindow,
    session: &ProfitWindow,
    recent: &[RecentTx],
    funnel: &crate::stats::ArbStatsSnapshot,
    bridge_breakdown: &[BridgeStatsSnapshot],
) -> String {
    let vault_line = match vault_xlm {
        Some(v) => format!("🏦 Vault XLM: `{}` XLM", format_xlm4_u(v)),
        None => "🏦 Vault XLM: `(n/a)`".to_string(),
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

    let grand = vault_xlm.unwrap_or(0).saturating_add(caller_total);

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

    format!(
        "📊 LumAgg Arb Monitor\n\
         \n\
         {vault_line}\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {funnel_block}\n\
         \n\
         {}\n\
         \n\
         {caller_block}\n\
         ✅ Grand Total: `{}` XLM",
        format_window("⏱ Last hour:", hour),
        format_window("📈 Session:", session),
        format_recent(recent),
        format_bridge_breakdown(bridge_breakdown),
        format_xlm4_u(grand),
    )
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
    fn report_contains_sections() {
        let hour = ProfitWindow {
            succeeded: 2,
            failed: 1,
            unknown: 0,
            submitted: 3,
            gross_profit_stroops: 500_000,
            fee_stroops: 1_000_000,
        };
        let session = hour.clone();
        let recent = vec![RecentTx {
            hash: "57132675d4897067".into(),
            amount_in: 100_000_000,
            gross_profit: 380_000,
            fee: 1_074_562,
        }];
        let msg = format_report(
            Some(18_000_000_000),
            &[(1, "GAAA".into(), Some(595_035_000))],
            &hour,
            &session,
            &recent,
            &empty_funnel(),
            &[],
        );
        assert!(msg.contains("LumAgg Arb Monitor"));
        assert!(msg.contains("Last hour"));
        assert!(msg.contains("57132675d489"));
        assert!(msg.contains("59.5035"));
        assert!(msg.contains("Quote→sim funnel"));
        assert!(msg.contains("avg quote−sim gap"));
        assert!(msg.contains("below_quoted=30"));
    }
}

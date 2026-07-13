//! LumAgg round-trip arbitrage bot (quote-api + aggregator.round_trip_swap).
//!
//!   ARB_BRIDGE_TOKENS=... ARB_AGGREGATOR_CONTRACT=C... ARB_QUOTE_API_URL=http://127.0.0.1:8080 \
//!   ARB_BUILD_TX=1 ARB_SUBMIT_TX=1 cargo run -p arbitrage --bin arb-scanner

use {
    anyhow::Result,
    arbitrage::{telegram, ArbConfig, ArbRuntime},
    std::sync::Arc,
    tracing::info,
    tracing_subscriber::EnvFilter,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = ArbConfig::from_env()?;
    let runtime = Arc::new(ArbRuntime::from_config(config)?);
    runtime.log_startup();

    if let Some(alerter) = lumagg_alerts::TelegramAlerter::from_env().map(Arc::new) {
        info!("Telegram profit reports enabled");
        let _ = alerter
            .send("🚀 LumAgg arb-scanner started (hourly profit reports)")
            .await;
        telegram::spawn_hourly_profit_report(runtime.clone(), alerter);
    } else {
        info!("Telegram disabled (set TELEGRAM_ALERTS_ENABLED + token/chat in telegram.env)");
    }

    loop {
        let opps = arbitrage::scan_once(&runtime).await?;
        let stats = runtime.stats.snapshot();
        info!(
            round_opportunities = opps.len(),
            total_opportunities = stats.opportunities,
            prepared = stats.txs_prepared,
            sim_profit_rejected = stats.txs_sim_profit_rejected,
            submitted = stats.txs_submitted,
            succeeded = stats.txs_succeeded,
            failed = stats.txs_failed,
            dedup_skipped = stats.txs_dedup_skipped,
            dry_run = stats.txs_dry_run,
            "scan round finished"
        );

        if runtime.config.scan_interval_ms == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(runtime.config.scan_interval_ms)).await;
    }

    Ok(())
}

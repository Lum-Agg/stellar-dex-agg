//! LumAgg round-trip arbitrage bot (quote-api + aggregator.round_trip_swap).
//!
//! Burberry pipeline: BridgeCollector → parallel workers → async TxExecutor.
//!
//!   ARB_BRIDGE_TOKENS=... ARB_AGGREGATOR_CONTRACT=C... ARB_QUOTE_API_URL=http://127.0.0.1:8080 \
//!   ARB_BUILD_TX=1 ARB_SUBMIT_TX=1 cargo run -p arbitrage --bin arb-scanner

use {
    anyhow::Result,
    arbitrage::{pipeline::engine, telegram, ArbConfig, ArbRuntime},
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
        let _ = alerter.send("🚀 LumAgg arb-scanner started (burberry pipeline)").await;
        telegram::spawn_hourly_profit_report(runtime.clone(), alerter);
    } else {
        info!("Telegram disabled (set TELEGRAM_ALERTS_ENABLED + token/chat in telegram.env)");
    }

    engine::start_bot(runtime).await
}

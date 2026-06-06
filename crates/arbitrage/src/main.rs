//! LumAgg round-trip arbitrage bot (aggregator.round_trip_swap + split
//! routing).
//!
//!   ARB_BRIDGE_TOKENS=... ARB_AGGREGATOR_CONTRACT=C... \
//!   ARB_BUILD_TX=1 ARB_SUBMIT_TX=1 SNAPSHOT_REDIS_URL=... \
//!   cargo run -p arbitrage --bin arb-scanner

use {
    anyhow::Result,
    arbitrage::{ArbConfig, ArbRuntime},
    tracing::info,
    tracing_subscriber::EnvFilter,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = ArbConfig::from_env()?;
    let runtime = ArbRuntime::from_config(config)?;
    runtime.log_startup();

    loop {
        let opps = arbitrage::scan_once(&runtime).await?;
        let stats = runtime.stats.snapshot();
        info!(
            round_opportunities = opps.len(),
            total_opportunities = stats.opportunities,
            prepared = stats.txs_prepared,
            submitted = stats.txs_submitted,
            succeeded = stats.txs_succeeded,
            failed = stats.txs_failed,
            dedup_skipped = stats.txs_dedup_skipped,
            dry_run = stats.txs_dry_run,
            "scan round finished"
        );

        if runtime.config.scan_interval_secs == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(runtime.config.scan_interval_secs)).await;
    }

    Ok(())
}

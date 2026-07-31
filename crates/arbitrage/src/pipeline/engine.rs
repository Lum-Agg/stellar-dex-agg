//! Burberry engine wiring (collector → strategy → executor).

use {
    crate::{
        pipeline::{
            collector::BridgeCollector,
            executor::TxExecutor,
            strategy::BridgeStrategy,
            types::{Action, Event},
        },
        runtime::SharedRuntime,
        stats::spawn_stats_reporter,
    },
    anyhow::Result,
    burberry::{map_collector, map_executor, Engine},
    std::time::Duration,
    tracing::info,
};

pub async fn start_bot(runtime: SharedRuntime) -> Result<()> {
    let worker_count = runtime.config.worker_count;
    let item_gap_ms = runtime.config.item_gap_ms;
    let cycle_pause_ms = runtime.config.scan_interval_ms;

    info!(
        worker_count,
        item_gap_ms,
        cycle_pause_ms,
        bridges = runtime.config.bridge_tokens.len(),
        "starting burberry arb engine"
    );

    spawn_xlm_usdc_price_refresher(runtime.clone());

    let mut engine = Engine::default();

    engine.add_collector(map_collector!(
        BridgeCollector::new(runtime.config.clone(), runtime.stats.clone()),
        Event::BridgeScan
    ));

    engine.add_strategy(Box::new(BridgeStrategy::new(runtime.clone(), worker_count)));

    engine.add_executor(map_executor!(
        TxExecutor::new(runtime.clone()),
        Action::ExecuteOpportunity
    ));

    spawn_stats_reporter(runtime.clone(), Duration::from_secs(300));

    engine
        .run_and_join()
        .await
        .map_err(|e| anyhow::anyhow!("burberry engine: {e}"))?;

    Ok(())
}

fn spawn_xlm_usdc_price_refresher(runtime: SharedRuntime) {
    let refresh_secs = runtime.config.xlm_usdc_price_refresh_secs;
    if refresh_secs == 0 {
        info!(
            fallback_e7 = runtime.config.xlm_usdc_price_e7,
            "XLM/USDC live refresh disabled — using ARB_XLM_USDC_PRICE_E7 fallback only"
        );
        return;
    }

    tokio::spawn(async move {
        let price = runtime.xlm_usdc_price.clone();
        let client = runtime.quote_client.clone();
        let interval = Duration::from_secs(refresh_secs.max(15));
        loop {
            if let Err(err) = price.refresh(&client).await {
                tracing::warn!(error = %err, fallback_e7 = price.get(), "XLM/USDC mark refresh failed");
            }
            tokio::time::sleep(interval).await;
        }
    });
}

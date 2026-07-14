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

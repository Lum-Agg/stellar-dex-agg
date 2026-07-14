//! Yield base×bridge pairs continuously (stellar-arb RandomArbCollector
//! pattern).

use {
    crate::{config::ArbConfig, pipeline::types::BridgeScanItem, stats::ArbStats},
    anyhow::Result,
    async_trait::async_trait,
    burberry::{Collector, CollectorStream},
    std::{
        sync::{atomic::Ordering, Arc},
        time::Duration,
    },
    tracing::debug,
};

pub struct BridgeCollector {
    config: ArbConfig,
    stats: Arc<ArbStats>,
}

impl BridgeCollector {
    pub fn new(config: ArbConfig, stats: Arc<ArbStats>) -> Self {
        Self { config, stats }
    }
}

#[async_trait]
impl Collector<BridgeScanItem> for BridgeCollector {
    async fn get_event_stream(&self) -> Result<CollectorStream<'_, BridgeScanItem>> {
        let bases = self.config.base_tokens.clone();
        let bridges = self.config.bridge_tokens.clone();
        let item_gap = Duration::from_millis(self.config.item_gap_ms);
        let cycle_pause = Duration::from_millis(self.config.scan_interval_ms);
        let stats = self.stats.clone();

        let stream = async_stream::stream! {
            loop {
                for base in &bases {
                    for bridge in &bridges {
                        if base.canonical() == bridge.canonical() {
                            continue;
                        }
                        stats.routes_evaluated.fetch_add(1, Ordering::Relaxed);
                        debug!(
                            base = %base.canonical(),
                            bridge = %bridge.canonical(),
                            "bridge scan item"
                        );
                        yield BridgeScanItem {
                            base: base.clone(),
                            bridge: bridge.clone(),
                        };
                        if !item_gap.is_zero() {
                            tokio::time::sleep(item_gap).await;
                        }
                    }
                }
                if !cycle_pause.is_zero() {
                    tokio::time::sleep(cycle_pause).await;
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

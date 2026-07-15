//! Fire-and-forget tx submit (stellar-arb TxExecutor pattern).

use {
    crate::{
        dedup::round_trip_dedup_key, execute::try_execute_opportunity, runtime::SharedRuntime, scanner::ArbOpportunity,
    },
    anyhow::Result,
    async_trait::async_trait,
    burberry::Executor,
    tracing::warn,
};

pub struct TxExecutor {
    runtime: SharedRuntime,
}

impl TxExecutor {
    pub fn new(runtime: SharedRuntime) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Executor<ArbOpportunity> for TxExecutor {
    fn name(&self) -> &str {
        "TxExecutor"
    }

    async fn execute(&self, opp: ArbOpportunity) -> Result<()> {
        let runtime = self.runtime.clone();

        tokio::spawn(async move {
            let Some(pool) = &runtime.caller_pool else {
                return;
            };

            let ctx = match runtime.connect().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(route = %opp.route_label, error = %e, "executor connect failed");
                    return;
                }
            };

            let path_key = round_trip_dedup_key(&opp.quote.base, &opp.quote.bridge);
            if runtime.submit_enabled() {
                let mut cache = runtime.path_cache.lock().await;
                if cache.recently_submitted(&path_key) {
                    runtime
                        .stats
                        .txs_dedup_skipped
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }

            match try_execute_opportunity(
                &ctx,
                &opp,
                pool,
                runtime.stats.clone(),
                runtime.profit.clone(),
                runtime.dry_run(),
            )
            .await
            {
                Ok(true) if runtime.submit_enabled() => {
                    // Only lock the pair after a successful broadcast, so failed
                    // sims / busy callers do not burn the dedup window.
                    let mut cache = runtime.path_cache.lock().await;
                    cache.mark_submitted(path_key);
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(route = %opp.route_label, error = %e, "round_trip_swap pipeline failed");
                }
            }
        });

        Ok(())
    }
}

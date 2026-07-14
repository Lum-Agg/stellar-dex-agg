//! Lightweight counters for bot observability.

use {
    crate::runtime::SharedRuntime,
    std::{
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    },
    tracing::info,
};

#[derive(Debug, Default)]
pub struct ArbStats {
    pub routes_evaluated: AtomicU64,
    pub opportunities: AtomicU64,
    pub txs_prepared: AtomicU64,
    pub txs_sim_rejected: AtomicU64,
    pub txs_sim_profit_rejected: AtomicU64,
    pub txs_dry_run: AtomicU64,
    pub txs_submitted: AtomicU64,
    pub txs_succeeded: AtomicU64,
    pub txs_failed: AtomicU64,
    pub txs_dedup_skipped: AtomicU64,
}

impl ArbStats {
    pub fn snapshot(&self) -> ArbStatsSnapshot {
        ArbStatsSnapshot {
            routes_evaluated: self.routes_evaluated.load(Ordering::Relaxed),
            opportunities: self.opportunities.load(Ordering::Relaxed),
            txs_prepared: self.txs_prepared.load(Ordering::Relaxed),
            txs_sim_rejected: self.txs_sim_rejected.load(Ordering::Relaxed),
            txs_sim_profit_rejected: self.txs_sim_profit_rejected.load(Ordering::Relaxed),
            txs_dry_run: self.txs_dry_run.load(Ordering::Relaxed),
            txs_submitted: self.txs_submitted.load(Ordering::Relaxed),
            txs_succeeded: self.txs_succeeded.load(Ordering::Relaxed),
            txs_failed: self.txs_failed.load(Ordering::Relaxed),
            txs_dedup_skipped: self.txs_dedup_skipped.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArbStatsSnapshot {
    pub routes_evaluated: u64,
    pub opportunities: u64,
    pub txs_prepared: u64,
    pub txs_sim_rejected: u64,
    pub txs_sim_profit_rejected: u64,
    pub txs_dry_run: u64,
    pub txs_submitted: u64,
    pub txs_succeeded: u64,
    pub txs_failed: u64,
    pub txs_dedup_skipped: u64,
}

pub fn spawn_stats_reporter(runtime: SharedRuntime, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        tick.tick().await;
        loop {
            tick.tick().await;
            let s = runtime.stats.snapshot();
            info!(
                routes_evaluated = s.routes_evaluated,
                opportunities = s.opportunities,
                prepared = s.txs_prepared,
                sim_rejected = s.txs_sim_rejected,
                sim_profit_rejected = s.txs_sim_profit_rejected,
                submitted = s.txs_submitted,
                succeeded = s.txs_succeeded,
                failed = s.txs_failed,
                dedup_skipped = s.txs_dedup_skipped,
                dry_run = s.txs_dry_run,
                "arb stats summary"
            );
        }
    });
}

//! Lightweight counters for bot observability.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct ArbStats {
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

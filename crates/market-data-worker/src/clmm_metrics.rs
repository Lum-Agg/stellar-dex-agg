//! Counters for CLMM pool refresh vs Redis publish (complete coverage only).

use {
    market_snapshot::{pool_state_store::should_publish_clmm_to_redis, ClmmPoolSnapshot},
    std::sync::atomic::{AtomicU64, Ordering},
};

#[derive(Default)]
pub struct ClmmCoverageMetrics {
    pub refresh_attempts: AtomicU64,
    pub publish_skipped_incomplete: AtomicU64,
    pub published_complete: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClmmCoverageMetricsSnapshot {
    pub refresh_attempts: u64,
    pub publish_skipped_incomplete: u64,
    pub published_complete: u64,
}

impl ClmmCoverageMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_snapshot(&self, pool: &ClmmPoolSnapshot) {
        self.refresh_attempts.fetch_add(1, Ordering::Relaxed);
        if should_publish_clmm_to_redis(pool) {
            self.published_complete.fetch_add(1, Ordering::Relaxed);
        } else {
            self.publish_skipped_incomplete.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_snapshots(&self, pools: &[ClmmPoolSnapshot]) {
        for pool in pools {
            self.record_snapshot(pool);
        }
    }

    pub fn snapshot(&self) -> ClmmCoverageMetricsSnapshot {
        ClmmCoverageMetricsSnapshot {
            refresh_attempts: self.refresh_attempts.load(Ordering::Relaxed),
            publish_skipped_incomplete: self.publish_skipped_incomplete.load(Ordering::Relaxed),
            published_complete: self.published_complete.load(Ordering::Relaxed),
        }
    }

    pub fn skip_rate_bps(snapshot: ClmmCoverageMetricsSnapshot) -> u64 {
        if snapshot.refresh_attempts == 0 {
            return 0;
        }
        snapshot.publish_skipped_incomplete * 10_000 / snapshot.refresh_attempts
    }
}

#[cfg(test)]
mod tests {
    use {super::*, market_snapshot::ClmmCoverageSnapshot};

    fn sample_pool(complete: bool) -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "pool-1".to_string(),
            token0: "a".to_string(),
            token1: "b".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 1,
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: complete,
                min_loaded_tick: Some(-100),
                max_loaded_tick: Some(100),
                scanned_word_start: Some(-1),
                scanned_word_end: Some(1),
            }),
        }
    }

    #[test]
    fn records_complete_and_incomplete() {
        let metrics = ClmmCoverageMetrics::new();
        metrics.record_snapshot(&sample_pool(true));
        metrics.record_snapshot(&sample_pool(false));
        let snap = metrics.snapshot();
        assert_eq!(snap.refresh_attempts, 2);
        assert_eq!(snap.published_complete, 1);
        assert_eq!(snap.publish_skipped_incomplete, 1);
        assert_eq!(ClmmCoverageMetrics::skip_rate_bps(snap), 5000);
    }
}

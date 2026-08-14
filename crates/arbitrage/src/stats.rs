//! Lightweight counters for bot observability.
//!
//! Tracks the quote → simulate funnel, including why sims discard
//! (`size_unprofitable` / `below_quoted` / `fee_gate`) and rolling
//! quote-vs-on-chain gap_bps for long-term monitoring.

use {
    crate::runtime::SharedRuntime,
    std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicI64, AtomicU64, Ordering},
            Mutex,
        },
        time::Duration,
    },
    tracing::info,
};

#[derive(Debug, Default)]
pub struct ArbStats {
    /// Per-bridge funnel counters. Kept separate from the scalar snapshot so
    /// the hot-path reporter remains cheap and backwards-compatible.
    pub bridge: Mutex<BTreeMap<String, BridgeStats>>,
    pub routes_evaluated: AtomicU64,
    pub quote_failed: AtomicU64,
    pub unprofitable_quotes: AtomicU64,
    pub opportunities: AtomicU64,
    pub txs_prepared: AtomicU64,
    pub txs_sim_rejected: AtomicU64,
    /// All economic sim discards (size / below_quoted / fee / probe).
    pub txs_sim_profit_rejected: AtomicU64,
    /// Optimized size failed break-even on-chain (may still retry probe).
    pub discard_size_unprofitable: AtomicU64,
    /// Probe/sized route executed on-chain but below quoted profit.
    pub discard_below_quoted: AtomicU64,
    /// Sim succeeded but net after fees < ARB_MIN_PROFIT.
    pub discard_fee_gate: AtomicU64,
    /// Probe re-quote profit < min_profit (never reached chain sim for probe).
    pub discard_probe_unprofitable: AtomicU64,
    /// Sum of (quoted_bps − on_chain_bps) samples for avg gap.
    pub quote_sim_gap_bps_sum: AtomicI64,
    pub quote_sim_gap_samples: AtomicU64,
    pub txs_dry_run: AtomicU64,
    pub txs_submitted: AtomicU64,
    pub txs_succeeded: AtomicU64,
    pub txs_failed: AtomicU64,
    pub txs_dedup_skipped: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BridgeStats {
    pub evaluated: u64,
    pub quote_failed: u64,
    pub unprofitable_quotes: u64,
    pub opportunities: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeStatsSnapshot {
    pub bridge: String,
    pub evaluated: u64,
    pub quote_failed: u64,
    pub unprofitable_quotes: u64,
    pub opportunities: u64,
}

impl ArbStats {
    fn with_bridge(&self, bridge: &str, update: impl FnOnce(&mut BridgeStats)) {
        let mut by_bridge = self.bridge.lock().expect("arb bridge stats mutex poisoned");
        update(by_bridge.entry(bridge.to_owned()).or_default());
    }

    pub fn record_bridge_evaluated(&self, bridge: &str) {
        self.with_bridge(bridge, |stats| stats.evaluated = stats.evaluated.saturating_add(1));
    }

    pub fn record_bridge_quote_failed(&self, bridge: &str) {
        self.with_bridge(bridge, |stats| stats.quote_failed = stats.quote_failed.saturating_add(1));
    }

    pub fn record_bridge_unprofitable(&self, bridge: &str) {
        self.with_bridge(bridge, |stats| {
            stats.unprofitable_quotes = stats.unprofitable_quotes.saturating_add(1)
        });
    }

    pub fn record_bridge_opportunity(&self, bridge: &str) {
        self.with_bridge(bridge, |stats| stats.opportunities = stats.opportunities.saturating_add(1));
    }

    pub fn bridge_breakdown(&self) -> Vec<BridgeStatsSnapshot> {
        let by_bridge = self.bridge.lock().expect("arb bridge stats mutex poisoned");
        by_bridge
            .iter()
            .map(|(bridge, stats)| BridgeStatsSnapshot {
                bridge: bridge.clone(),
                evaluated: stats.evaluated,
                quote_failed: stats.quote_failed,
                unprofitable_quotes: stats.unprofitable_quotes,
                opportunities: stats.opportunities,
            })
            .collect()
    }

    pub fn snapshot(&self) -> ArbStatsSnapshot {
        let gap_samples = self.quote_sim_gap_samples.load(Ordering::Relaxed);
        let gap_sum = self.quote_sim_gap_bps_sum.load(Ordering::Relaxed);
        let avg_gap_bps = if gap_samples == 0 {
            0
        } else {
            gap_sum / gap_samples as i64
        };
        ArbStatsSnapshot {
            routes_evaluated: self.routes_evaluated.load(Ordering::Relaxed),
            quote_failed: self.quote_failed.load(Ordering::Relaxed),
            unprofitable_quotes: self.unprofitable_quotes.load(Ordering::Relaxed),
            opportunities: self.opportunities.load(Ordering::Relaxed),
            txs_prepared: self.txs_prepared.load(Ordering::Relaxed),
            txs_sim_rejected: self.txs_sim_rejected.load(Ordering::Relaxed),
            txs_sim_profit_rejected: self.txs_sim_profit_rejected.load(Ordering::Relaxed),
            discard_size_unprofitable: self.discard_size_unprofitable.load(Ordering::Relaxed),
            discard_below_quoted: self.discard_below_quoted.load(Ordering::Relaxed),
            discard_fee_gate: self.discard_fee_gate.load(Ordering::Relaxed),
            discard_probe_unprofitable: self.discard_probe_unprofitable.load(Ordering::Relaxed),
            avg_quote_sim_gap_bps: avg_gap_bps,
            quote_sim_gap_samples: gap_samples,
            txs_dry_run: self.txs_dry_run.load(Ordering::Relaxed),
            txs_submitted: self.txs_submitted.load(Ordering::Relaxed),
            txs_succeeded: self.txs_succeeded.load(Ordering::Relaxed),
            txs_failed: self.txs_failed.load(Ordering::Relaxed),
            txs_dedup_skipped: self.txs_dedup_skipped.load(Ordering::Relaxed),
        }
    }

    /// Record a quote-vs-on-chain gap sample (bps). Positive ⇒ quote more
    /// optimistic.
    pub fn record_quote_sim_gap(&self, amount_in: u128, quoted_amount_out: u128, on_chain_base_out: u128) {
        let gap = quote_sim_gap_bps(amount_in, quoted_amount_out, on_chain_base_out);
        self.quote_sim_gap_bps_sum.fetch_add(gap, Ordering::Relaxed);
        self.quote_sim_gap_samples.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArbStatsSnapshot {
    pub routes_evaluated: u64,
    pub quote_failed: u64,
    pub unprofitable_quotes: u64,
    pub opportunities: u64,
    pub txs_prepared: u64,
    pub txs_sim_rejected: u64,
    pub txs_sim_profit_rejected: u64,
    pub discard_size_unprofitable: u64,
    pub discard_below_quoted: u64,
    pub discard_fee_gate: u64,
    pub discard_probe_unprofitable: u64,
    pub avg_quote_sim_gap_bps: i64,
    pub quote_sim_gap_samples: u64,
    pub txs_dry_run: u64,
    pub txs_submitted: u64,
    pub txs_succeeded: u64,
    pub txs_failed: u64,
    pub txs_dedup_skipped: u64,
}

impl ArbStatsSnapshot {
    /// Return counters observed since the previous reporter tick.
    ///
    /// Runtime counters are monotonic for the lifetime of the process. Using
    /// saturating subtraction also keeps reporting safe across a restart or
    /// any future counter reset.
    pub fn delta_since(&self, previous: &Self) -> ArbStatsDelta {
        ArbStatsDelta {
            routes_evaluated: self.routes_evaluated.saturating_sub(previous.routes_evaluated),
            quote_failed: self.quote_failed.saturating_sub(previous.quote_failed),
            unprofitable_quotes: self
                .unprofitable_quotes
                .saturating_sub(previous.unprofitable_quotes),
            opportunities: self.opportunities.saturating_sub(previous.opportunities),
            txs_prepared: self.txs_prepared.saturating_sub(previous.txs_prepared),
            txs_sim_rejected: self.txs_sim_rejected.saturating_sub(previous.txs_sim_rejected),
            txs_sim_profit_rejected: self
                .txs_sim_profit_rejected
                .saturating_sub(previous.txs_sim_profit_rejected),
            txs_submitted: self.txs_submitted.saturating_sub(previous.txs_submitted),
            txs_succeeded: self.txs_succeeded.saturating_sub(previous.txs_succeeded),
            txs_failed: self.txs_failed.saturating_sub(previous.txs_failed),
            txs_dedup_skipped: self.txs_dedup_skipped.saturating_sub(previous.txs_dedup_skipped),
            quote_sim_gap_samples: self
                .quote_sim_gap_samples
                .saturating_sub(previous.quote_sim_gap_samples),
        }
    }

    pub fn prepare_rate_bps(&self) -> u64 {
        if self.opportunities == 0 {
            return 0;
        }
        (self.txs_prepared.saturating_mul(10_000)) / self.opportunities
    }

    pub fn sim_reject_rate_bps(&self) -> u64 {
        if self.opportunities == 0 {
            return 0;
        }
        (self.txs_sim_profit_rejected.saturating_mul(10_000)) / self.opportunities
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArbStatsDelta {
    pub routes_evaluated: u64,
    pub quote_failed: u64,
    pub unprofitable_quotes: u64,
    pub opportunities: u64,
    pub txs_prepared: u64,
    pub txs_sim_rejected: u64,
    pub txs_sim_profit_rejected: u64,
    pub txs_submitted: u64,
    pub txs_succeeded: u64,
    pub txs_failed: u64,
    pub txs_dedup_skipped: u64,
    pub quote_sim_gap_samples: u64,
}

/// quoted_bps − on_chain_bps. Positive means the quote path looked better than
/// sim.
pub fn quote_sim_gap_bps(amount_in: u128, quoted_amount_out: u128, on_chain_base_out: u128) -> i64 {
    let quoted = crate::scanner::compute_profit_bps(amount_in, quoted_amount_out);
    let on_chain = crate::scanner::compute_profit_bps(amount_in, on_chain_base_out);
    quoted.saturating_sub(on_chain)
}

/// Detect long stretches with opportunities but zero prepares (quote/sim funnel
/// leak).
#[derive(Debug, Clone)]
pub struct QuietWindowTracker {
    /// Consecutive quiet ticks required before alert.
    pub min_windows: u32,
    /// Minimum opportunity delta in a tick to count as "active scanning".
    pub min_opportunities: u64,
    consecutive_quiet: u32,
    last: ArbStatsSnapshot,
    primed: bool,
}

impl QuietWindowTracker {
    pub fn new(min_windows: u32, min_opportunities: u64) -> Self {
        Self {
            min_windows: min_windows.max(1),
            min_opportunities: min_opportunities.max(1),
            consecutive_quiet: 0,
            last: ArbStatsSnapshot {
                routes_evaluated: 0,
                quote_failed: 0,
                unprofitable_quotes: 0,
                opportunities: 0,
                txs_prepared: 0,
                txs_sim_rejected: 0,
                txs_sim_profit_rejected: 0,
                discard_size_unprofitable: 0,
                discard_below_quoted: 0,
                discard_fee_gate: 0,
                discard_probe_unprofitable: 0,
                avg_quote_sim_gap_bps: 0,
                quote_sim_gap_samples: 0,
                txs_dry_run: 0,
                txs_submitted: 0,
                txs_succeeded: 0,
                txs_failed: 0,
                txs_dedup_skipped: 0,
            },
            primed: false,
        }
    }

    pub fn from_env() -> Self {
        let min_windows = std::env::var("ARB_QUIET_ALERT_WINDOWS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5u32);
        let min_opportunities = std::env::var("ARB_QUIET_ALERT_MIN_OPPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50u64);
        Self::new(min_windows, min_opportunities)
    }

    /// Feed a new cumulative snapshot. Returns `Some(QuietWindowAlert)` when
    /// `min_windows` consecutive ticks saw opportunities but zero prepares.
    pub fn observe(&mut self, now: ArbStatsSnapshot) -> Option<QuietWindowAlert> {
        if !self.primed {
            self.last = now;
            self.primed = true;
            return None;
        }

        let d_opp = now.opportunities.saturating_sub(self.last.opportunities);
        let d_prep = now.txs_prepared.saturating_sub(self.last.txs_prepared);
        let d_size = now
            .discard_size_unprofitable
            .saturating_sub(self.last.discard_size_unprofitable);
        let d_below = now.discard_below_quoted.saturating_sub(self.last.discard_below_quoted);
        let d_fee = now.discard_fee_gate.saturating_sub(self.last.discard_fee_gate);
        let d_gap_samples = now
            .quote_sim_gap_samples
            .saturating_sub(self.last.quote_sim_gap_samples);

        self.last = now;

        let quiet = d_opp >= self.min_opportunities && d_prep == 0;
        if quiet {
            self.consecutive_quiet = self.consecutive_quiet.saturating_add(1);
        } else {
            self.consecutive_quiet = 0;
            return None;
        }

        if self.consecutive_quiet < self.min_windows {
            return None;
        }

        // Fire once per quiet streak tip (every tick after threshold).
        Some(QuietWindowAlert {
            consecutive_windows: self.consecutive_quiet,
            opportunities_delta: d_opp,
            prepared_delta: d_prep,
            discard_size_unprofitable_delta: d_size,
            discard_below_quoted_delta: d_below,
            discard_fee_gate_delta: d_fee,
            avg_quote_sim_gap_bps: now.avg_quote_sim_gap_bps,
            gap_samples_delta: d_gap_samples,
            prepare_rate_bps: now.prepare_rate_bps(),
            sim_reject_rate_bps: now.sim_reject_rate_bps(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuietWindowAlert {
    pub consecutive_windows: u32,
    pub opportunities_delta: u64,
    pub prepared_delta: u64,
    pub discard_size_unprofitable_delta: u64,
    pub discard_below_quoted_delta: u64,
    pub discard_fee_gate_delta: u64,
    pub avg_quote_sim_gap_bps: i64,
    pub gap_samples_delta: u64,
    pub prepare_rate_bps: u64,
    pub sim_reject_rate_bps: u64,
}

impl QuietWindowAlert {
    pub fn telegram_text(&self) -> String {
        format!(
            "⚠️ LumAgg arb quiet window\n\
             · consecutive ticks: {}\n\
             · Δ opportunities: {} (prepared: {})\n\
             · Δ discards: size={} below_quoted={} fee_gate={}\n\
             · avg quote−sim gap: {} bps (session)\n\
             · prepare_rate: {} bps · sim_reject_rate: {} bps\n\
             Likely quote path optimistic vs on-chain sim — check pool freshness.",
            self.consecutive_windows,
            self.opportunities_delta,
            self.prepared_delta,
            self.discard_size_unprofitable_delta,
            self.discard_below_quoted_delta,
            self.discard_fee_gate_delta,
            self.avg_quote_sim_gap_bps,
            self.prepare_rate_bps,
            self.sim_reject_rate_bps,
        )
    }
}

pub fn spawn_stats_reporter(runtime: SharedRuntime, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        tick.tick().await;
        let mut previous = runtime.stats.snapshot();
        loop {
            tick.tick().await;
            let s = runtime.stats.snapshot();
            let delta = s.delta_since(&previous);
            previous = s;
            info!(
                routes_evaluated = s.routes_evaluated,
                opportunities = s.opportunities,
                prepared = s.txs_prepared,
                prepare_rate_bps = s.prepare_rate_bps(),
                sim_rejected = s.txs_sim_rejected,
                sim_profit_rejected = s.txs_sim_profit_rejected,
                sim_reject_rate_bps = s.sim_reject_rate_bps(),
                discard_size_unprofitable = s.discard_size_unprofitable,
                discard_below_quoted = s.discard_below_quoted,
                discard_fee_gate = s.discard_fee_gate,
                discard_probe_unprofitable = s.discard_probe_unprofitable,
                avg_quote_sim_gap_bps = s.avg_quote_sim_gap_bps,
                quote_sim_gap_samples = s.quote_sim_gap_samples,
                submitted = s.txs_submitted,
                succeeded = s.txs_succeeded,
                failed = s.txs_failed,
                dedup_skipped = s.txs_dedup_skipped,
                dry_run = s.txs_dry_run,
                delta_routes_evaluated = delta.routes_evaluated,
                delta_opportunities = delta.opportunities,
                delta_prepared = delta.txs_prepared,
                delta_sim_rejected = delta.txs_sim_rejected,
                delta_sim_profit_rejected = delta.txs_sim_profit_rejected,
                delta_submitted = delta.txs_submitted,
                delta_succeeded = delta.txs_succeeded,
                delta_failed = delta.txs_failed,
                delta_dedup_skipped = delta.txs_dedup_skipped,
                delta_quote_sim_gap_samples = delta.quote_sim_gap_samples,
                bridge_breakdown = ?runtime.stats.bridge_breakdown(),
                "arb stats summary"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_sim_gap_bps_matches_observed_20bps_pattern() {
        // Probe 1 XLM: quote +14.3 bps, chain −5.8 bps → gap ≈ 20 bps
        let amount_in = 100_000_000u128;
        let quoted = 100_143_095u128;
        let on_chain = 99_942_226u128;
        let gap = quote_sim_gap_bps(amount_in, quoted, on_chain);
        assert!((19..=21).contains(&gap), "gap={gap}");
    }

    #[test]
    fn quiet_tracker_requires_consecutive_windows() {
        let mut t = QuietWindowTracker::new(2, 10);
        let mut snap = ArbStatsSnapshot {
            routes_evaluated: 0,
            quote_failed: 0,
            unprofitable_quotes: 0,
            opportunities: 0,
            txs_prepared: 0,
            txs_sim_rejected: 0,
            txs_sim_profit_rejected: 0,
            discard_size_unprofitable: 0,
            discard_below_quoted: 0,
            discard_fee_gate: 0,
            discard_probe_unprofitable: 0,
            avg_quote_sim_gap_bps: 20,
            quote_sim_gap_samples: 0,
            txs_dry_run: 0,
            txs_submitted: 0,
            txs_succeeded: 0,
            txs_failed: 0,
            txs_dedup_skipped: 0,
        };
        assert!(t.observe(snap).is_none()); // prime

        snap.opportunities = 50;
        assert!(t.observe(snap).is_none()); // window 1

        snap.opportunities = 100;
        let alert = t.observe(snap).expect("window 2 fires");
        assert_eq!(alert.consecutive_windows, 2);
        assert_eq!(alert.opportunities_delta, 50);
        assert_eq!(alert.prepared_delta, 0);

        // Prepare breaks the streak.
        snap.opportunities = 150;
        snap.txs_prepared = 1;
        assert!(t.observe(snap).is_none());
        assert_eq!(t.consecutive_quiet, 0);
    }

    #[test]
    fn quiet_tracker_ignores_low_opportunity_ticks() {
        let mut t = QuietWindowTracker::new(1, 100);
        let mut snap = ArbStatsSnapshot {
            routes_evaluated: 0,
            quote_failed: 0,
            unprofitable_quotes: 0,
            opportunities: 0,
            txs_prepared: 0,
            txs_sim_rejected: 0,
            txs_sim_profit_rejected: 0,
            discard_size_unprofitable: 0,
            discard_below_quoted: 0,
            discard_fee_gate: 0,
            discard_probe_unprofitable: 0,
            avg_quote_sim_gap_bps: 0,
            quote_sim_gap_samples: 0,
            txs_dry_run: 0,
            txs_submitted: 0,
            txs_succeeded: 0,
            txs_failed: 0,
            txs_dedup_skipped: 0,
        };
        assert!(t.observe(snap).is_none());
        snap.opportunities = 50; // below threshold
        assert!(t.observe(snap).is_none());
    }

    #[test]
    fn rates_handle_zero_opportunities() {
        let s = ArbStats::default().snapshot();
        assert_eq!(s.prepare_rate_bps(), 0);
        assert_eq!(s.sim_reject_rate_bps(), 0);
    }

    #[test]
    fn snapshot_delta_tracks_only_new_counters() {
        let mut previous = ArbStatsSnapshot {
            opportunities: 10,
            txs_prepared: 2,
            txs_succeeded: 1,
            ..ArbStats::default().snapshot()
        };
        let current = ArbStatsSnapshot {
            opportunities: 15,
            txs_prepared: 3,
            txs_succeeded: 2,
            ..previous
        };

        assert_eq!(
            current.delta_since(&previous),
            ArbStatsDelta {
                opportunities: 5,
                txs_prepared: 1,
                txs_succeeded: 1,
                ..ArbStatsDelta::default()
            }
        );

        previous.opportunities = 0;
        assert_eq!(current.delta_since(&previous).opportunities, 15);
    }

    #[test]
    fn bridge_breakdown_tracks_each_bridge() {
        let stats = ArbStats::default();
        stats.record_bridge_evaluated("A");
        stats.record_bridge_evaluated("A");
        stats.record_bridge_quote_failed("A");
        stats.record_bridge_opportunity("B");

        assert_eq!(
            stats.bridge_breakdown(),
            vec![
                BridgeStatsSnapshot {
                    bridge: "A".into(),
                    evaluated: 2,
                    quote_failed: 1,
                    unprofitable_quotes: 0,
                    opportunities: 0,
                },
                BridgeStatsSnapshot {
                    bridge: "B".into(),
                    evaluated: 0,
                    quote_failed: 0,
                    unprofitable_quotes: 0,
                    opportunities: 1,
                },
            ]
        );
    }
}

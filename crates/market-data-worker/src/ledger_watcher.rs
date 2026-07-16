//! Poll `getLatestLedger` + `getEvents` and return pools touched since the last
//! cursor.

use {
    anyhow::Result,
    dex_adapters::{
        pool_index::{touched_pools_from_events, KnownPoolIndex, PoolRef},
        rpc::{events::EventFilterSpec, SorobanRpc},
    },
    market_snapshot::{ClmmPoolSnapshot, SourceSnapshot},
    std::collections::HashSet,
    tracing::{debug, info, warn},
};

/// Default ledger poll interval (seconds, fractional OK via env).
pub const DEFAULT_LEDGER_POLL_SECS: f64 = 0.1;
pub const MIN_LEDGER_POLL_SECS: f64 = 0.1;
pub const DEFAULT_LEDGER_MAX_CATCHUP: u32 = 32;
pub const DEFAULT_LEDGER_MAX_TOUCHED_REFRESH: usize = 64;

pub fn ledger_poll_duration_from_env() -> std::time::Duration {
    let secs = std::env::var("LEDGER_POLL_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(DEFAULT_LEDGER_POLL_SECS)
        .max(MIN_LEDGER_POLL_SECS);
    std::time::Duration::from_secs_f64(secs)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::sync::{Mutex, OnceLock},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn ledger_poll_duration_parses_fractional_seconds() {
        let _guard = env_lock().lock().unwrap();
        let original = std::env::var("LEDGER_POLL_SECS").ok();
        std::env::set_var("LEDGER_POLL_SECS", "0.5");
        assert_eq!(ledger_poll_duration_from_env().as_millis(), 500);
        match original {
            Some(value) => std::env::set_var("LEDGER_POLL_SECS", value),
            None => std::env::remove_var("LEDGER_POLL_SECS"),
        }
    }
}

pub fn ledger_watcher_enabled_from_env() -> bool {
    std::env::var("LEDGER_WATCHER_ENABLED")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true)
}

pub fn ledger_max_touched_refresh_from_env() -> usize {
    std::env::var("LEDGER_MAX_TOUCHED_REFRESH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LEDGER_MAX_TOUCHED_REFRESH)
        .max(1)
}

/// Tracks ledger sequence and ingests contract events for known pool contracts.
pub struct LedgerWatcher {
    rpc: SorobanRpc,
    last_ledger: Option<u32>,
    max_catchup_ledgers: u32,
}

impl LedgerWatcher {
    pub fn new(rpc: SorobanRpc) -> Self {
        Self {
            rpc,
            last_ledger: None,
            max_catchup_ledgers: std::env::var("LEDGER_MAX_CATCHUP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_LEDGER_MAX_CATCHUP)
                .max(1),
        }
    }

    pub async fn bootstrap(&mut self) -> Result<()> {
        let latest = self.rpc.get_latest_ledger().await?.sequence;
        self.last_ledger = Some(latest.saturating_sub(1));
        info!(latest, "Ledger watcher bootstrapped");
        Ok(())
    }

    /// Poll for new ledgers and return pools that emitted contract events.
    pub async fn poll_touched_pools(&mut self, index: &KnownPoolIndex) -> Result<HashSet<PoolRef>> {
        let latest = self.rpc.get_latest_ledger().await?.sequence;
        let Some(cursor) = self.last_ledger else {
            self.last_ledger = Some(latest.saturating_sub(1));
            return Ok(HashSet::new());
        };

        if latest <= cursor {
            return Ok(HashSet::new());
        }

        let mut start = cursor + 1;
        let end_inclusive = latest;
        let span = end_inclusive.saturating_sub(start) + 1;
        if span > self.max_catchup_ledgers {
            // After restart we may be hundreds of ledgers behind; fetch each ledger
            // individually but only the most recent window (LEDGER_MAX_CATCHUP).
            let skipped = span - self.max_catchup_ledgers;
            start = end_inclusive.saturating_sub(self.max_catchup_ledgers - 1);
            warn!(
                skipped,
                max = self.max_catchup_ledgers,
                latest,
                "Ledger backlog truncated — ingesting last window per ledger"
            );
        }

        // One getEvents call per ledger (startLedger=N, endLedger=N+1). When the poll
        // interval sees multiple new ledgers, each is fetched separately so pagination
        // limits apply per block, not across the whole catch-up span.
        let filters = vec![EventFilterSpec {
            contract_ids: None,
            topics: Some(vec![vec!["**".to_string()]]),
        }];

        let mut touched = HashSet::new();
        let mut total_events = 0usize;
        for ledger in start..=end_inclusive {
            let events = self
                .rpc
                .get_contract_events(
                    ledger,
                    Some(ledger + 1),
                    &filters,
                    dex_adapters::rpc::events::DEFAULT_EVENTS_PAGE_LIMIT,
                )
                .await?;
            total_events += events.len();
            touched.extend(touched_pools_from_events(&events, index));
        }

        self.last_ledger = Some(latest);
        let ledger_count = end_inclusive - start + 1;
        if !touched.is_empty() {
            info!(
                ledger_count,
                start,
                end = end_inclusive,
                events = total_events,
                touched = touched.len(),
                "Ledger watcher ingested per-ledger events"
            );
        } else if ledger_count > 1 {
            debug!(
                ledger_count,
                start,
                end = end_inclusive,
                events = total_events,
                "Ledger watcher polled multiple ledgers, no touched pools"
            );
        }
        Ok(touched)
    }
}

pub fn rebuild_pool_index(sources: &[SourceSnapshot], clmm_pools: &[ClmmPoolSnapshot]) -> KnownPoolIndex {
    let refs = market_snapshot::MarketSnapshot::clmm_pool_refs_from_states(clmm_pools);
    KnownPoolIndex::rebuild(sources, &refs)
}

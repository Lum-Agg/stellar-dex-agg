//! Poll `getLatestLedger` + `getEvents` and return pools touched since the last cursor.

use std::collections::HashSet;

use anyhow::Result;
use dex_adapters::{
    pool_index::{KnownPoolIndex, PoolRef, touched_pools_from_events},
    rpc::{
        events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
        SorobanRpc,
    },
};
use market_snapshot::{ClmmPoolSnapshot, SourceSnapshot};
use tracing::{debug, info, warn};

pub const DEFAULT_LEDGER_POLL_SECS: u64 = 3;
pub const DEFAULT_LEDGER_MAX_CATCHUP: u32 = 32;
pub const DEFAULT_LEDGER_MAX_TOUCHED_REFRESH: usize = 64;

pub fn ledger_poll_secs_from_env() -> u64 {
    std::env::var("LEDGER_POLL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LEDGER_POLL_SECS)
        .max(1)
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

    pub fn last_ledger(&self) -> Option<u32> {
        self.last_ledger
    }

    /// Poll for new ledgers and return pools that emitted contract events.
    pub async fn poll_touched_pools(
        &mut self,
        index: &KnownPoolIndex,
    ) -> Result<HashSet<PoolRef>> {
        let latest = self.rpc.get_latest_ledger().await?.sequence;
        let Some(cursor) = self.last_ledger else {
            self.last_ledger = Some(latest.saturating_sub(1));
            return Ok(HashSet::new());
        };

        if latest <= cursor {
            return Ok(HashSet::new());
        }

        let mut start = cursor + 1;
        let end = latest + 1; // exclusive
        let span = end - start;
        if span > self.max_catchup_ledgers {
            start = end.saturating_sub(self.max_catchup_ledgers);
            warn!(
                skipped = span - self.max_catchup_ledgers,
                "Ledger catch-up truncated to max_catchup"
            );
        }
        if span > MAX_LEDGER_SCAN_PER_REQUEST {
            start = end.saturating_sub(MAX_LEDGER_SCAN_PER_REQUEST);
            warn!(
                "Ledger span capped to RPC getEvents max {}",
                MAX_LEDGER_SCAN_PER_REQUEST
            );
        }

        // All contract events in range (Soroswap swaps emit from pair contracts).
        let filters = vec![EventFilterSpec {
            contract_ids: None,
            topics: Some(vec![vec!["**".to_string()]]),
        }];

        let events = self
            .rpc
            .get_contract_events(start, Some(end), &filters, dex_adapters::rpc::events::DEFAULT_EVENTS_PAGE_LIMIT)
            .await?;

        self.last_ledger = Some(latest);
        let touched = touched_pools_from_events(&events, index);
        if !touched.is_empty() {
            debug!(
                ledgers = format!("{start}..{end}"),
                events = events.len(),
                touched = touched.len(),
                "Ledger watcher found touched pools"
            );
        }
        Ok(touched)
    }
}

pub fn rebuild_pool_index(
    sources: &[SourceSnapshot],
    clmm_pools: &[ClmmPoolSnapshot],
) -> KnownPoolIndex {
    KnownPoolIndex::rebuild(sources, clmm_pools)
}

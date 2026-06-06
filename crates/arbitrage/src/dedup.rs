//! Suppress duplicate submissions for the same pool path within one ledger
//! window.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Default ~1 Stellar ledger (~5s); stellar-arb uses 6s.
pub const DEFAULT_DEDUP_TTL: Duration = Duration::from_secs(6);

pub struct SubmittedPathCache {
    ttl: Duration,
    entries: HashMap<String, Instant>,
}

impl SubmittedPathCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn prune_expired(&mut self) {
        let ttl = self.ttl;
        self.entries.retain(|_, ts| ts.elapsed() < ttl);
    }

    /// Returns true if this path was submitted recently.
    pub fn recently_submitted(&mut self, path_key: &str) -> bool {
        self.prune_expired();
        self.entries
            .get(path_key)
            .map(|ts| ts.elapsed() < self.ttl)
            .unwrap_or(false)
    }

    pub fn mark_submitted(&mut self, path_key: String) {
        self.prune_expired();
        self.entries.insert(path_key, Instant::now());
    }
}

pub fn path_dedup_key(pool_addresses: &[String]) -> String {
    pool_addresses.join("|")
}

pub fn round_trip_dedup_key(base: &router_engine::TokenId, bridge: &router_engine::TokenId) -> String {
    format!("{}|{}", base.canonical(), bridge.canonical())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_blocks_repeat_path() {
        let mut cache = SubmittedPathCache::new(Duration::from_secs(60));
        let key = path_dedup_key(&["p1".into(), "p2".into()]);
        assert!(!cache.recently_submitted(&key));
        cache.mark_submitted(key.clone());
        assert!(cache.recently_submitted(&key));
    }
}

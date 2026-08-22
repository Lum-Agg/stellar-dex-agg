//! Suppress duplicate submissions for the same pool path within one ledger
//! window.

use {
    crate::bridge::RoundTripQuote,
    std::{
        collections::HashMap,
        time::{Duration, Instant},
    },
};

/// Default ~1 Stellar ledger (~5s); stellar-arb uses 6s.
pub const DEFAULT_DEDUP_TTL: Duration = Duration::from_secs(6);
const RESERVATION_TTL: Duration = Duration::from_secs(120);

enum PathState {
    Reserved(Instant),
    Submitted(Instant),
}

pub struct SubmittedPathCache {
    ttl: Duration,
    entries: HashMap<String, PathState>,
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
        self.entries.retain(|_, state| match state {
            PathState::Reserved(ts) => ts.elapsed() < RESERVATION_TTL,
            PathState::Submitted(ts) => ts.elapsed() < ttl,
        });
    }

    /// Returns true if this path was submitted or reserved recently.
    pub fn recently_submitted(&mut self, path_key: &str) -> bool {
        self.prune_expired();
        self.entries.contains_key(path_key)
    }

    /// Atomically reserve a path for one execution attempt.
    pub fn try_reserve(&mut self, path_key: String) -> bool {
        if self.recently_submitted(&path_key) {
            return false;
        }
        self.entries.insert(path_key, PathState::Reserved(Instant::now()));
        true
    }

    /// Release a reservation when no transaction was broadcast.
    pub fn release(&mut self, path_key: &str) {
        self.entries.remove(path_key);
    }

    pub fn mark_submitted(&mut self, path_key: String) {
        self.prune_expired();
        self.entries.insert(path_key, PathState::Submitted(Instant::now()));
    }
}

pub fn path_dedup_key(pool_addresses: &[String]) -> String {
    pool_addresses.join("|")
}

/// Build a key for the exact out-leg and back-leg route. Different routes may
/// share a pool, but should not suppress each other solely for that reason.
pub fn round_trip_dedup_key(quote: &RoundTripQuote) -> String {
    fn leg_key(leg: &crate::quote_client::LegQuote) -> String {
        leg.route
            .sub_orders
            .iter()
            .map(|sub_order| {
                format!(
                    "{}:{}",
                    sub_order.path.sources.join(","),
                    sub_order.path.pool_addresses.join(">")
                )
            })
            .collect::<Vec<_>>()
            .join("||")
    }

    format!(
        "{}|{}|{}",
        quote.base.canonical(),
        leg_key(&quote.leg_out),
        leg_key(&quote.leg_back)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_blocks_repeat_path() {
        let mut cache = SubmittedPathCache::new(Duration::from_secs(60));
        let key = path_dedup_key(&["p1".into(), "p2".into()]);
        assert!(cache.try_reserve(key.clone()));
        assert!(!cache.try_reserve(key.clone()));
        assert!(cache.recently_submitted(&key));
        cache.release(&key);
        assert!(cache.try_reserve(key.clone()));
        cache.mark_submitted(key.clone());
        assert!(cache.recently_submitted(&key));
    }
}

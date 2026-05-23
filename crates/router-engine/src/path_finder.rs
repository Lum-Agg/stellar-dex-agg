//! Path finder: discovers and caches trading paths across all DEX sources.

use crate::{
    graph::TokenGraph,
    types::{Path, TokenId, TradingPair},
};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::info;

/// Configuration for path finding.
#[derive(Debug, Clone)]
pub struct PathFinderConfig {
    /// Maximum hops per path (default: 3)
    pub max_hops: usize,
    /// Maximum indirect paths (2+ hops). Direct pools between token_in/out are enumerated separately.
    pub max_multi_hop_paths: usize,
    /// Cap on direct (1-hop) pools between the pair (`0` = include all direct pools in the graph).
    pub max_direct_paths: usize,
    /// Bridge tokens used to improve multi-hop discovery
    /// (high-liquidity tokens like XLM, USDC that connect many pairs)
    pub bridge_tokens: Vec<TokenId>,
}

impl Default for PathFinderConfig {
    fn default() -> Self {
        Self {
            max_hops: 3,
            max_multi_hop_paths: 50,
            max_direct_paths: 0,
            bridge_tokens: vec![
                TokenId::Native, // XLM
                TokenId::Classic {
                    code: "USDC".into(),
                    issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
                },
            ],
        }
    }
}

/// Path finder maintains the token graph and discovers paths.
pub struct PathFinder {
    graph: TokenGraph,
    config: PathFinderConfig,
    /// Path cache — separate mutex so `find_paths` only needs a read lock on the finder.
    cache: Mutex<HashMap<(String, String), CachedPaths>>,
}

struct CachedPaths {
    paths: Vec<Path>,
    cached_at_ms: u64,
}

/// Cache TTL: paths are valid for 30 seconds
const CACHE_TTL_MS: u64 = 30_000;

/// Excluded from quote path discovery (aggregator Comet execution not deployed yet).
const QUOTE_EXCLUDED_SOURCES: &[&str] = &["comet"];

impl PathFinder {
    pub fn new(config: PathFinderConfig) -> Self {
        Self {
            graph: TokenGraph::new(),
            config,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Update the graph with trading pairs from a DEX source.
    /// Replaces all existing edges from that source.
    pub fn update_from_source(&mut self, source: &str, pairs: &[TradingPair]) {
        // Remove old edges from this source
        self.graph.remove_source(source);

        if QUOTE_EXCLUDED_SOURCES.contains(&source) {
            self.cache.lock().unwrap().clear();
            info!(
                source = source,
                pairs = pairs.len(),
                "Skipping quote graph edges for excluded DEX source"
            );
            return;
        }

        // Add new edges
        for pair in pairs {
            self.graph.add_pair(
                &pair.token_a,
                &pair.token_b,
                source,
                &pair.pool_address,
                pair.fee_bps,
            );
        }

        // Invalidate all cached paths (source changed)
        self.cache.lock().unwrap().clear();

        info!(
            source = source,
            pairs = pairs.len(),
            total_tokens = self.graph.token_count(),
            total_edges = self.graph.edge_count(),
            "Token graph updated"
        );
    }

    /// Find all valid paths from token_in to token_out.
    pub fn find_paths(&self, token_in: &TokenId, token_out: &TokenId) -> Vec<Path> {
        self.find_paths_with_limits(
            token_in,
            token_out,
            self.config.max_hops,
            self.config.max_multi_hop_paths,
            self.config.max_direct_paths,
        )
    }

    pub fn default_max_hops(&self) -> usize {
        self.config.max_hops
    }

    pub fn default_max_multi_hop_paths(&self) -> usize {
        self.config.max_multi_hop_paths
    }

    pub fn default_max_direct_paths(&self) -> usize {
        self.config.max_direct_paths
    }

    pub fn find_paths_with_limits(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        max_hops: usize,
        max_multi_hop_paths: usize,
        max_direct_paths: usize,
    ) -> Vec<Path> {
        let cache_key = (token_in.canonical(), token_out.canonical());
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let use_cache = max_hops == self.config.max_hops
            && max_multi_hop_paths == self.config.max_multi_hop_paths
            && max_direct_paths == self.config.max_direct_paths;
        if use_cache {
            if let Ok(cache) = self.cache.lock() {
                if let Some(cached) = cache.get(&cache_key) {
                    if now - cached.cached_at_ms < CACHE_TTL_MS {
                        return cached.paths.clone();
                    }
                }
            }
        }

        let paths = self.graph.find_paths(
            token_in,
            token_out,
            max_hops,
            max_multi_hop_paths,
            max_direct_paths,
        );

        if use_cache {
            if let Ok(mut cache) = self.cache.lock() {
                cache.insert(
                    cache_key,
                    CachedPaths {
                        paths: paths.clone(),
                        cached_at_ms: now,
                    },
                );
            }
        }

        paths
    }

    /// Invalidate cached paths involving a specific token pair.
    pub fn invalidate(&mut self, token_a: &TokenId, token_b: &TokenId) {
        let key_a = token_a.canonical();
        let key_b = token_b.canonical();
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|k, _| k.0 != key_a && k.0 != key_b && k.1 != key_a && k.1 != key_b);
        }
    }

    /// Clear all caches.
    pub fn clear_cache(&mut self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Get graph stats.
    pub fn stats(&self) -> (usize, usize) {
        (self.graph.token_count(), self.graph.edge_count())
    }
}

//! Pool data cache: persists pool data to disk so the server can start
//! instantly.
//!
//! On startup:
//! 1. Load cached pools from disk (instant)
//! 2. Register adapters with cached data
//! 3. Background task refreshes from chain and updates cache
//!
//! Cache file format: JSON array of AdapterTradingPair per source.

use {
    crate::traits::AdapterTradingPair,
    anyhow::Result,
    serde::{Deserialize, Serialize},
    std::path::Path,
    tracing::info,
};

/// Cache entry for a single DEX source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSource {
    pub source: String,
    pub pairs: Vec<AdapterTradingPair>,
    pub updated_at: u64, // unix timestamp ms
}

/// Full cache containing all sources.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolCache {
    pub sources: Vec<CachedSource>,
}

impl PoolCache {
    /// Load cache from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let cache: PoolCache = serde_json::from_str(&content)?;
        info!(
            "Loaded pool cache: {} sources, {} total pairs",
            cache.sources.len(),
            cache.sources.iter().map(|s| s.pairs.len()).sum::<usize>()
        );
        Ok(cache)
    }

    /// Save cache to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        info!(
            "Saved pool cache: {} sources, {} total pairs",
            self.sources.len(),
            self.sources.iter().map(|s| s.pairs.len()).sum::<usize>()
        );
        Ok(())
    }

    /// Get pairs for a specific source.
    pub fn get_pairs(&self, source: &str) -> Option<&[AdapterTradingPair]> {
        self.sources
            .iter()
            .find(|s| s.source == source)
            .map(|s| s.pairs.as_slice())
    }

    /// Update pairs for a specific source.
    pub fn update_source(&mut self, source: &str, pairs: Vec<AdapterTradingPair>) {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        if let Some(existing) = self.sources.iter_mut().find(|s| s.source == source) {
            existing.pairs = pairs;
            existing.updated_at = now;
        } else {
            self.sources.push(CachedSource {
                source: source.to_string(),
                pairs,
                updated_at: now,
            });
        }
    }

    /// Check if cache is stale (older than max_age_secs).
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        self.sources.iter().any(|s| (now - s.updated_at) / 1000 > max_age_secs)
    }
}

/// Default cache file path.
pub fn default_cache_path() -> std::path::PathBuf {
    std::path::PathBuf::from("data/pool_cache.json")
}

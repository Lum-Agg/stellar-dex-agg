//! Application configuration loaded from environment variables or config file.

use {
    market_snapshot::store::{DEFAULT_REDIS_EVENTS_CHANNEL, DEFAULT_REDIS_SNAPSHOT_HISTORY},
    serde::{Deserialize, Serialize},
};

fn normalize_snapshot_poll_interval_ms(interval_ms: u64) -> u64 {
    interval_ms.max(1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Soroban RPC endpoint URL
    pub rpc_url: String,
    /// Network passphrase
    pub network_passphrase: String,
    /// API server listen address
    pub listen_addr: String,
    /// Aggregator contract address (optional, for on-chain execution)
    pub aggregator_contract: Option<String>,
    /// Pool reserve refresh interval (seconds). Keep short so quotes track live
    /// reserves.
    pub refresh_interval_secs: u64,
    /// Full pool discovery interval (seconds): re-run `get_trading_pairs` and
    /// replace the graph.
    pub discovery_interval_secs: u64,
    /// Price impact threshold (bps) above which split optimization is
    /// attempted.
    pub split_threshold_bps: u32,
    /// Also try split when the second-best path is within this delta (bps) of
    /// the best path.
    pub split_competitive_delta_bps: u32,
    /// Drop split legs whose expected output is below this share of total
    /// output.
    pub min_split_fraction_bps: u32,
    /// Maximum number of candidate paths to consider for split optimization.
    pub max_splits: usize,
    /// Path finder: max hops per path (direct pools are always enumerated
    /// separately).
    pub path_finder_max_hops: usize,
    /// Path finder: cap on 2+ hop paths per quote.
    pub path_finder_max_multi_hop_paths: usize,
    /// Path finder: cap on 1-hop pools (`0` = all direct pools in graph).
    pub path_finder_max_direct_paths: usize,
    /// Allow API to RPC-fetch xy=k pool misses (default false — worker writes
    /// Redis).
    pub quote_rpc_hydrate_enabled: bool,
    /// Max xy=k pools to RPC-fetch per quote when `quote_rpc_hydrate_enabled`
    /// is true.
    pub quote_hydrate_max_pools: usize,
    /// Optional snapshot backend selector (`file` or `redis`). When unset,
    /// snapshot mode is enabled only if `snapshot_dir` is set.
    pub snapshot_backend: Option<String>,
    /// Optional directory containing file-backed market snapshots.
    pub snapshot_dir: Option<String>,
    /// Redis URL for shared snapshot storage.
    pub snapshot_redis_url: Option<String>,
    /// Redis Pub/Sub channel used to accelerate snapshot reloads.
    pub snapshot_redis_channel: String,
    /// Number of latest snapshot versions to retain in Redis history.
    pub snapshot_redis_keep_latest: usize,
    /// Poll interval for snapshot reload checks.
    pub snapshot_poll_interval_ms: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string(),
            network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
            listen_addr: "0.0.0.0:3100".to_string(),
            aggregator_contract: None,
            refresh_interval_secs: 5,
            discovery_interval_secs: 600,
            split_threshold_bps: 5,
            split_competitive_delta_bps: 50,
            min_split_fraction_bps: 5,
            max_splits: 5,
            path_finder_max_hops: 3,
            path_finder_max_multi_hop_paths: 50,
            path_finder_max_direct_paths: 0,
            quote_rpc_hydrate_enabled: false,
            quote_hydrate_max_pools: 12,
            snapshot_backend: None,
            snapshot_dir: None,
            snapshot_redis_url: None,
            snapshot_redis_channel: DEFAULT_REDIS_EVENTS_CHANNEL.to_string(),
            snapshot_redis_keep_latest: DEFAULT_REDIS_SNAPSHOT_HISTORY,
            snapshot_poll_interval_ms: 1_000,
        }
    }
}

impl AppConfig {
    /// Load config from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| Self::default().rpc_url),
            network_passphrase: std::env::var("NETWORK_PASSPHRASE")
                .unwrap_or_else(|_| Self::default().network_passphrase),
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| Self::default().listen_addr),
            aggregator_contract: std::env::var("AGGREGATOR_CONTRACT").ok(),
            refresh_interval_secs: std::env::var("REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            discovery_interval_secs: std::env::var("DISCOVERY_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(600),
            split_threshold_bps: std::env::var("SPLIT_THRESHOLD_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().split_threshold_bps),
            split_competitive_delta_bps: std::env::var("SPLIT_COMPETITIVE_DELTA_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().split_competitive_delta_bps),
            min_split_fraction_bps: std::env::var("MIN_SPLIT_FRACTION_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().min_split_fraction_bps),
            max_splits: std::env::var("MAX_SPLITS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().max_splits),
            path_finder_max_hops: std::env::var("PATH_FINDER_MAX_HOPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_hops),
            path_finder_max_multi_hop_paths: std::env::var("PATH_FINDER_MAX_MULTI_HOP_PATHS")
                .or_else(|_| std::env::var("PATH_FINDER_MAX_PATHS"))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_multi_hop_paths),
            path_finder_max_direct_paths: std::env::var("PATH_FINDER_MAX_DIRECT_PATHS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_direct_paths),
            quote_rpc_hydrate_enabled: std::env::var("QUOTE_RPC_HYDRATE_ENABLED")
                .ok()
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(Self::default().quote_rpc_hydrate_enabled),
            quote_hydrate_max_pools: std::env::var("QUOTE_HYDRATE_MAX_POOLS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().quote_hydrate_max_pools),
            snapshot_backend: std::env::var("SNAPSHOT_BACKEND").ok(),
            snapshot_dir: std::env::var("SNAPSHOT_DIR").ok(),
            snapshot_redis_url: std::env::var("SNAPSHOT_REDIS_URL").ok(),
            snapshot_redis_channel: std::env::var("SNAPSHOT_REDIS_CHANNEL")
                .unwrap_or_else(|_| Self::default().snapshot_redis_channel),
            snapshot_redis_keep_latest: std::env::var("SNAPSHOT_REDIS_KEEP_LATEST")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().snapshot_redis_keep_latest),
            snapshot_poll_interval_ms: normalize_snapshot_poll_interval_ms(
                std::env::var("SNAPSHOT_POLL_INTERVAL_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(Self::default().snapshot_poll_interval_ms),
            ),
        }
    }
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
    fn default_config_uses_default_snapshot_redis_settings() {
        let config = AppConfig::default();

        assert_eq!(config.snapshot_redis_channel, "lumagg:snapshot:events");
        assert_eq!(config.snapshot_redis_keep_latest, 10);
    }

    #[test]
    fn from_env_reads_snapshot_redis_channel_and_keep_latest() {
        let _guard = env_lock().lock().unwrap();
        let original_channel = std::env::var("SNAPSHOT_REDIS_CHANNEL").ok();
        let original_keep_latest = std::env::var("SNAPSHOT_REDIS_KEEP_LATEST").ok();
        std::env::set_var("SNAPSHOT_REDIS_CHANNEL", "snapshots:test");
        std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", "42");

        let config = AppConfig::from_env();

        assert_eq!(config.snapshot_redis_channel, "snapshots:test");
        assert_eq!(config.snapshot_redis_keep_latest, 42);

        match original_channel {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_CHANNEL", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_CHANNEL"),
        }
        match original_keep_latest {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_KEEP_LATEST"),
        }
    }

    #[test]
    fn from_env_normalizes_zero_snapshot_poll_interval() {
        let _guard = env_lock().lock().unwrap();
        let original = std::env::var("SNAPSHOT_POLL_INTERVAL_MS").ok();
        std::env::set_var("SNAPSHOT_POLL_INTERVAL_MS", "0");

        let config = AppConfig::from_env();

        assert_eq!(config.snapshot_poll_interval_ms, 1);

        match original {
            Some(value) => std::env::set_var("SNAPSHOT_POLL_INTERVAL_MS", value),
            None => std::env::remove_var("SNAPSHOT_POLL_INTERVAL_MS"),
        }
    }
}

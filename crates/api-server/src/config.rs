//! Application configuration loaded from environment variables or config file.

use serde::{Deserialize, Serialize};

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
    /// Pool refresh interval in seconds
    pub refresh_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string(),
            network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
            listen_addr: "0.0.0.0:3100".to_string(),
            aggregator_contract: None,
            refresh_interval_secs: 60,
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
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| Self::default().listen_addr),
            aggregator_contract: std::env::var("AGGREGATOR_CONTRACT").ok(),
            refresh_interval_secs: std::env::var("REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        }
    }
}

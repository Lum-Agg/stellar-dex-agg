//! Shared helpers for Stellar Expert HTTP API (contract storage indexing).

use {
    anyhow::{anyhow, Result},
    reqwest::Client,
};

pub const STELLAR_EXPERT_API: &str = "https://api.stellar.expert/explorer/public";
const USER_AGENT: &str = "lumagg-dex-aggregator/1.0";

pub fn expert_http_client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| anyhow!("HTTP client: {}", e))
}

//! Arb runtime context: quote-api client.

use {
    crate::{config::ArbConfig, prepare::LatestLedgerCache, quote_client::QuoteApiClient},
    anyhow::Result,
    std::sync::Arc,
};

pub struct ArbContext {
    pub config: ArbConfig,
    pub quote_client: QuoteApiClient,
    /// Shared across prepares; vault allowance expiry does not need a fresh
    /// `getLatestLedger` every opportunity.
    pub latest_ledger: Arc<LatestLedgerCache>,
}

impl ArbContext {
    pub async fn connect(config: ArbConfig) -> Result<Self> {
        Self::connect_with_ledger_cache(config, Arc::new(LatestLedgerCache::new())).await
    }

    pub async fn connect_with_ledger_cache(config: ArbConfig, latest_ledger: Arc<LatestLedgerCache>) -> Result<Self> {
        Ok(Self {
            quote_client: QuoteApiClient::from_config(&config),
            config,
            latest_ledger,
        })
    }
}

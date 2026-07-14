//! Arb runtime context: quote-api client.

use {
    crate::{config::ArbConfig, quote_client::QuoteApiClient},
    anyhow::Result,
};

pub struct ArbContext {
    pub config: ArbConfig,
    pub quote_client: QuoteApiClient,
}

impl ArbContext {
    pub async fn connect(config: ArbConfig) -> Result<Self> {
        Ok(Self {
            quote_client: QuoteApiClient::from_config(&config),
            config,
        })
    }
}

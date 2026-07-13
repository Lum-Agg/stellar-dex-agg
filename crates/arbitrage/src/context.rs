//! Arb runtime context: quote-api client + vault float.

use {
    crate::{config::ArbConfig, quote_client::QuoteApiClient, vault::fetch_token_balance_stroops},
    anyhow::Result,
};

pub struct ArbContext {
    pub config: ArbConfig,
    pub quote_client: QuoteApiClient,
    /// Vault SAC balance for the primary base token (stroops), when vault mode
    /// is on.
    pub vault_base_balance: Option<u128>,
}

impl ArbContext {
    pub async fn connect(config: ArbConfig) -> Result<Self> {
        let quote_client = QuoteApiClient::from_config(&config);

        let vault_base_balance = match (config.vault_contract.as_deref(), config.base_tokens.first()) {
            (Some(vault), Some(base)) => {
                match fetch_token_balance_stroops(&config.rpc_url, &base.canonical(), vault).await {
                    Ok(bal) => {
                        tracing::info!(
                            vault,
                            base = %base.canonical(),
                            balance = bal,
                            "vault base float loaded"
                        );
                        Some(bal)
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            vault,
                            base = %base.canonical(),
                            "failed to read vault base balance; using config max_amount_in only"
                        );
                        None
                    }
                }
            }
            _ => None,
        };

        Ok(Self {
            config,
            quote_client,
            vault_base_balance,
        })
    }
}

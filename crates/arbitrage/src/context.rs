//! Arb runtime context: quote-api client.

use {
    crate::{
        config::ArbConfig, economics, prepare::LatestLedgerCache, quote_client::QuoteApiClient,
        vault::VaultBalanceCache, xlm_price::XlmUsdcPrice,
    },
    anyhow::Result,
    std::sync::Arc,
};

pub struct ArbContext {
    pub config: ArbConfig,
    pub quote_client: QuoteApiClient,
    /// Shared across prepares; vault allowance expiry does not need a fresh
    /// `getLatestLedger` every opportunity.
    pub latest_ledger: Arc<LatestLedgerCache>,
    /// Vault SAC balances for size caps (TTL cache).
    pub vault_balances: Arc<VaultBalanceCache>,
    /// Live (or fallback) XLM→USDC mark for USDC-base fee gates.
    pub xlm_usdc_price: Arc<XlmUsdcPrice>,
}

impl ArbContext {
    pub async fn connect(config: ArbConfig) -> Result<Self> {
        Self::connect_with_caches(
            config,
            Arc::new(LatestLedgerCache::new()),
            Arc::new(VaultBalanceCache::new()),
        )
        .await
    }

    pub async fn connect_with_ledger_cache(config: ArbConfig, latest_ledger: Arc<LatestLedgerCache>) -> Result<Self> {
        Self::connect_with_caches(config, latest_ledger, Arc::new(VaultBalanceCache::new())).await
    }

    pub async fn connect_with_caches(
        config: ArbConfig,
        latest_ledger: Arc<LatestLedgerCache>,
        vault_balances: Arc<VaultBalanceCache>,
    ) -> Result<Self> {
        let quote_client = QuoteApiClient::from_config(&config);
        let xlm_usdc_price = Arc::new(XlmUsdcPrice::new(config.xlm_usdc_price_e7));
        Self::connect_with_resources(config, latest_ledger, vault_balances, quote_client, xlm_usdc_price)
            .await
    }

    pub async fn connect_with_resources(
        config: ArbConfig,
        latest_ledger: Arc<LatestLedgerCache>,
        vault_balances: Arc<VaultBalanceCache>,
        quote_client: QuoteApiClient,
        xlm_usdc_price: Arc<XlmUsdcPrice>,
    ) -> Result<Self> {
        Ok(Self {
            quote_client,
            config,
            latest_ledger,
            vault_balances,
            xlm_usdc_price,
        })
    }

    /// Convert XLM resource-fee stroops into base-token units using the live mark.
    pub fn fee_in_base(&self, fee_xlm_stroops: u128, base_token: &str) -> u128 {
        economics::fee_in_base_units(fee_xlm_stroops, base_token, self.xlm_usdc_price.get())
    }
}

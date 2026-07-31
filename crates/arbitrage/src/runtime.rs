//! Bot runtime: config, caller pool, dedup, stats.

use {
    crate::{
        callers::CallerPool, config::ArbConfig, context::ArbContext, dedup::SubmittedPathCache,
        prepare::LatestLedgerCache, profit::ProfitBook, quote_client::QuoteApiClient, stats::ArbStats,
        vault::VaultBalanceCache, xlm_price::XlmUsdcPrice,
    },
    anyhow::Result,
    std::sync::Arc,
    tokio::sync::Mutex,
    tracing::info,
};

pub struct ArbRuntime {
    pub config: ArbConfig,
    pub stats: Arc<ArbStats>,
    pub profit: Arc<ProfitBook>,
    pub path_cache: Mutex<SubmittedPathCache>,
    pub caller_pool: Option<CallerPool>,
    pub latest_ledger: Arc<LatestLedgerCache>,
    pub vault_balances: Arc<VaultBalanceCache>,
    pub quote_client: QuoteApiClient,
    pub xlm_usdc_price: Arc<XlmUsdcPrice>,
}

impl ArbRuntime {
    pub fn from_config(config: ArbConfig) -> Result<Self> {
        let secret_keys = load_secret_keys(&config)?;
        let caller_pool =
            CallerPool::from_config(config.mnemonic_path.as_deref(), &config.caller_indices, &secret_keys)?;

        let dedup_secs = config.submit_dedup_secs;
        let quote_client = QuoteApiClient::from_config(&config);
        let xlm_usdc_price = Arc::new(XlmUsdcPrice::new(config.xlm_usdc_price_e7));
        Ok(Self {
            config,
            stats: Arc::new(ArbStats::default()),
            profit: Arc::new(ProfitBook::default()),
            path_cache: Mutex::new(SubmittedPathCache::new(std::time::Duration::from_secs(
                dedup_secs.max(1),
            ))),
            caller_pool,
            latest_ledger: Arc::new(LatestLedgerCache::new()),
            vault_balances: Arc::new(VaultBalanceCache::new()),
            quote_client,
            xlm_usdc_price,
        })
    }

    pub async fn connect(&self) -> Result<ArbContext> {
        ArbContext::connect_with_resources(
            self.config.clone(),
            self.latest_ledger.clone(),
            self.vault_balances.clone(),
            self.quote_client.clone(),
            self.xlm_usdc_price.clone(),
        )
        .await
    }

    pub fn build_enabled(&self) -> bool {
        self.config.build_tx && self.config.aggregator_contract.is_some() && self.has_callers()
    }

    pub fn submit_enabled(&self) -> bool {
        self.config.submit_tx && self.build_enabled()
    }

    pub fn dry_run(&self) -> bool {
        self.config.dry_run
    }

    pub fn has_callers(&self) -> bool {
        self.caller_pool.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
    }

    pub fn log_startup(&self) {
        let callers = self.caller_pool.as_ref().map(|p| p.len()).unwrap_or(0);
        info!(
            build_tx = self.build_enabled(),
            submit_tx = self.submit_enabled(),
            dry_run = self.dry_run(),
            callers,
            quote_api = ?self.config.quote_api_urls,
            aggregator = ?self.config.aggregator_contract,
            vault = ?self.config.vault_contract,
            bridges = self.config.bridge_tokens.len(),
            min_profit = self.config.min_profit,
            xlm_usdc_price_e7_fallback = self.config.xlm_usdc_price_e7,
            xlm_usdc_price_refresh_secs = self.config.xlm_usdc_price_refresh_secs,
            "arb runtime ready"
        );
        if self.config.build_tx && self.config.aggregator_contract.is_none() {
            tracing::warn!("ARB_BUILD_TX=1 but ARB_AGGREGATOR_CONTRACT unset");
        }
        if self.config.build_tx && self.config.vault_contract.is_some() {
            info!("vault mode: callers only need native XLM for fees; principal held in ARB_VAULT_CONTRACT");
        } else if self.config.build_tx && self.config.vault_contract.is_none() {
            info!("direct aggregator mode: bot wallets hold trade float");
        }
        if self.config.build_tx && !self.has_callers() {
            tracing::warn!(
                "no callers loaded — set ARB_SECRET_KEY, ARB_CALLER_SECRETS, or ARB_MNEMONIC_PATH + ARB_CALLER_INDICES"
            );
        }
    }
}

fn load_secret_keys(config: &ArbConfig) -> Result<Vec<String>> {
    let mut keys = config.caller_secrets.clone();
    if let Ok(s) = std::env::var("ARB_SECRET_KEY") {
        if !s.is_empty() {
            keys.push(s);
        }
    }
    if let Some(path) = &config.caller_secrets_file {
        let content = std::fs::read_to_string(path)?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                keys.push(line.to_string());
            }
        }
    }
    Ok(keys)
}

pub type SharedRuntime = std::sync::Arc<ArbRuntime>;

use {
    crate::{set, set_list, set_option},
    anyhow::{bail, Result},
    serde::Deserialize,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArbitrageConfig {
    pub network: Network,
    pub contracts: Contracts,
    pub accounts: Accounts,
    pub assets: Assets,
    #[serde(default)]
    pub scanner: Scanner,
    #[serde(default)]
    pub execution: Execution,
    #[serde(default)]
    pub monitoring: Monitoring,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub rpc_url: String,
    pub quote_api_urls: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contracts {
    pub aggregator: String,
    pub vault: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accounts {
    pub caller_secrets_file: Option<String>,
    pub mnemonic_path: Option<String>,
    #[serde(default = "default_indices")]
    pub caller_indices: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    #[serde(default)]
    pub base_tokens: Option<Vec<String>>,
    pub bridge_tokens: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scanner {
    pub probe_amount_in: Option<u128>,
    pub min_profit: Option<u128>,
    pub min_profit_xlm: Option<u128>,
    pub min_profit_usdc: Option<u128>,
    pub xlm_usdc_price_e7: Option<u128>,
    pub xlm_usdc_price_refresh_secs: Option<u64>,
    pub slippage_bps: Option<u32>,
    pub max_hops: Option<usize>,
    pub max_splits: Option<usize>,
    pub on_chain_validate: Option<bool>,
    pub scan_interval_ms: Option<u64>,
    pub item_gap_ms: Option<u64>,
    pub worker_count: Option<usize>,
    pub optimize_amount: Option<bool>,
    pub min_amount_in: Option<u128>,
    pub max_amount_in: Option<u128>,
    pub sample_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Execution {
    pub build_tx: Option<bool>,
    pub submit_tx: Option<bool>,
    pub dry_run: Option<bool>,
    pub poll_tx: Option<bool>,
    pub submit_dedup_secs: Option<u64>,
    pub caller_cooldown_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Monitoring {
    pub log_filter: Option<String>,
    pub telegram_enabled: Option<bool>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub telegram_interval_secs: Option<u64>,
    pub quiet_alert_tick_secs: Option<u64>,
    pub quiet_alert_cooldown_secs: Option<u64>,
    pub quiet_alert_windows: Option<usize>,
    pub quiet_alert_min_opportunities: Option<u64>,
}

impl ArbitrageConfig {
    pub fn validate(&self) -> Result<()> {
        if self.network.rpc_url.trim().is_empty() {
            bail!("network.rpc_url must not be empty");
        }
        if self.network.quote_api_urls.is_empty() {
            bail!("network.quote_api_urls must contain at least one URL");
        }
        if self.assets.bridge_tokens.is_empty() {
            bail!("assets.bridge_tokens must contain at least one token");
        }
        Ok(())
    }

    pub fn apply(&self) {
        set("RPC_URL", &self.network.rpc_url);
        set("ARB_QUOTE_API_URLS", self.network.quote_api_urls.join(","));
        set("ARB_AGGREGATOR_CONTRACT", &self.contracts.aggregator);
        set_option("ARB_VAULT_CONTRACT", &self.contracts.vault);
        set_option("ARB_CALLER_SECRETS_FILE", &self.accounts.caller_secrets_file);
        set_option("ARB_MNEMONIC_PATH", &self.accounts.mnemonic_path);
        set(
            "ARB_CALLER_INDICES",
            self.accounts
                .caller_indices
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        set_list("ARB_BASE_TOKENS", &self.assets.base_tokens);
        set("ARB_BRIDGE_TOKENS", self.assets.bridge_tokens.join(","));

        set_option("ARB_PROBE_AMOUNT_IN", &self.scanner.probe_amount_in);
        set_option("ARB_MIN_PROFIT", &self.scanner.min_profit);
        set_option("ARB_MIN_PROFIT_XLM", &self.scanner.min_profit_xlm);
        set_option("ARB_MIN_PROFIT_USDC", &self.scanner.min_profit_usdc);
        set_option("ARB_XLM_USDC_PRICE_E7", &self.scanner.xlm_usdc_price_e7);
        set_option(
            "ARB_XLM_USDC_PRICE_REFRESH_SECS",
            &self.scanner.xlm_usdc_price_refresh_secs,
        );
        set_option("ARB_SLIPPAGE_BPS", &self.scanner.slippage_bps);
        set_option("ARB_MAX_HOPS", &self.scanner.max_hops);
        set_option("ARB_MAX_SPLITS", &self.scanner.max_splits);
        set_option("ARB_ON_CHAIN_VALIDATE", &self.scanner.on_chain_validate);
        set_option("ARB_SCAN_INTERVAL_MS", &self.scanner.scan_interval_ms);
        set_option("ARB_ITEM_GAP_MS", &self.scanner.item_gap_ms);
        set_option("ARB_WORKER_COUNT", &self.scanner.worker_count);
        set_option("ARB_OPTIMIZE_AMOUNT", &self.scanner.optimize_amount);
        set_option("ARB_MIN_AMOUNT_IN", &self.scanner.min_amount_in);
        set_option("ARB_MAX_AMOUNT_IN", &self.scanner.max_amount_in);
        set_option("ARB_SAMPLE_COUNT", &self.scanner.sample_count);

        set_option("ARB_BUILD_TX", &self.execution.build_tx);
        set_option("ARB_SUBMIT_TX", &self.execution.submit_tx);
        set_option("ARB_DRY_RUN", &self.execution.dry_run);
        set_option("ARB_POLL_TX", &self.execution.poll_tx);
        set_option("ARB_SUBMIT_DEDUP_SECS", &self.execution.submit_dedup_secs);
        set_option("ARB_CALLER_COOLDOWN_MS", &self.execution.caller_cooldown_ms);

        set_option("RUST_LOG", &self.monitoring.log_filter);
        set_option("TELEGRAM_ALERTS_ENABLED", &self.monitoring.telegram_enabled);
        set_option("TELEGRAM_BOT_TOKEN", &self.monitoring.telegram_bot_token);
        set_option("TELEGRAM_CHAT_ID", &self.monitoring.telegram_chat_id);
        set_option("ARB_TELEGRAM_INTERVAL_SECS", &self.monitoring.telegram_interval_secs);
        set_option("ARB_QUIET_ALERT_TICK_SECS", &self.monitoring.quiet_alert_tick_secs);
        set_option(
            "ARB_QUIET_ALERT_COOLDOWN_SECS",
            &self.monitoring.quiet_alert_cooldown_secs,
        );
        set_option("ARB_QUIET_ALERT_WINDOWS", &self.monitoring.quiet_alert_windows);
        set_option(
            "ARB_QUIET_ALERT_MIN_OPPS",
            &self.monitoring.quiet_alert_min_opportunities,
        );
    }
}

fn default_indices() -> Vec<u32> {
    vec![0]
}

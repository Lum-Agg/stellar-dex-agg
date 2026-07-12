//! Environment configuration for the arbitrage scanner.

use {
    anyhow::{Context, Result},
    router_engine::TokenId,
};

/// Default hub tokens (mainnet contract IDs).
const DEFAULT_BASE_TOKENS: &[&str] = &[
    "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA", // XLM
    "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC
];

#[derive(Debug, Clone)]
pub struct ArbConfig {
    pub snapshot_redis_url: String,
    pub pool_state_redis_url: String,
    /// Deployed LumAgg aggregator contract (round_trip_swap target).
    pub aggregator_contract: Option<String>,
    /// Optional arb vault; when set, txs call vault.execute_round_trip instead
    /// of aggregator directly.
    pub vault_contract: Option<String>,
    pub caller_secrets: Vec<String>,
    pub caller_secrets_file: Option<String>,
    pub mnemonic_path: Option<String>,
    pub caller_indices: Vec<u32>,
    pub rpc_url: String,
    pub horizon_url: String,
    pub base_tokens: Vec<TokenId>,
    pub bridge_tokens: Vec<TokenId>,
    pub probe_amount_in: u128,
    /// Minimum round-trip profit in base-token stroops (7 decimals for XLM).
    pub min_profit: u128,
    pub slippage_bps: u32,
    pub max_hops: usize,
    pub max_splits: usize,
    /// Delay between scan rounds (milliseconds).
    pub scan_interval_ms: u64,
    pub build_tx: bool,
    pub optimize_amount: bool,
    pub min_amount_in: u128,
    pub max_amount_in: u128,
    pub sample_count: usize,
    pub submit_tx: bool,
    pub dry_run: bool,
    pub submit_dedup_secs: u64,
}

impl ArbConfig {
    pub fn from_env() -> Result<Self> {
        let snapshot_redis_url = std::env::var("SNAPSHOT_REDIS_URL")
            .or_else(|_| std::env::var("REDIS_URL"))
            .context("SNAPSHOT_REDIS_URL or REDIS_URL required")?;
        let pool_state_redis_url = std::env::var("POOL_STATE_REDIS_URL").unwrap_or_else(|_| snapshot_redis_url.clone());

        let aggregator_contract = std::env::var("ARB_AGGREGATOR_CONTRACT")
            .or_else(|_| std::env::var("AGGREGATOR_CONTRACT"))
            .ok()
            .filter(|s| !s.is_empty());

        let vault_contract = std::env::var("ARB_VAULT_CONTRACT").ok().filter(|s| !s.is_empty());

        let caller_secrets = std::env::var("ARB_CALLER_SECRETS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let caller_secrets_file = std::env::var("ARB_CALLER_SECRETS_FILE").ok().filter(|s| !s.is_empty());

        let mnemonic_path = std::env::var("ARB_MNEMONIC_PATH").ok().filter(|s| !s.is_empty());

        let caller_indices = std::env::var("ARB_CALLER_INDICES")
            .ok()
            .map(|raw| raw.split(',').filter_map(|s| s.trim().parse().ok()).collect())
            .unwrap_or_else(|| vec![0]);

        let rpc_url =
            std::env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());
        let horizon_url = std::env::var("HORIZON_URL").unwrap_or_else(|_| "https://horizon.stellar.org".to_string());

        let base_tokens = std::env::var("ARB_BASE_TOKENS")
            .ok()
            .map(|raw| parse_token_list(&raw))
            .unwrap_or_else(|| DEFAULT_BASE_TOKENS.iter().map(|s| TokenId::from_str_auto(s)).collect());

        let bridge_tokens = std::env::var("ARB_BRIDGE_TOKENS")
            .ok()
            .map(|raw| parse_token_list(&raw))
            .filter(|v| !v.is_empty())
            .context("ARB_BRIDGE_TOKENS required (comma-separated intermediate tokens)")?;

        let probe_amount_in: u128 = std::env::var("ARB_PROBE_AMOUNT_IN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000_000); // 10 XLM

        let min_profit = std::env::var("ARB_MIN_PROFIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000); // 0.01 XLM

        let slippage_bps = std::env::var("ARB_SLIPPAGE_BPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let max_hops = std::env::var("ARB_MAX_HOPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);

        let max_splits = std::env::var("ARB_MAX_SPLITS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let scan_interval_ms = std::env::var("ARB_SCAN_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("ARB_SCAN_INTERVAL_SECS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|s| s.saturating_mul(1000))
            })
            .unwrap_or(500);

        let build_tx = std::env::var("ARB_BUILD_TX")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let optimize_amount = std::env::var("ARB_OPTIMIZE_AMOUNT")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);

        let min_amount_in = std::env::var("ARB_MIN_AMOUNT_IN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(probe_amount_in);

        let max_amount_in = std::env::var("ARB_MAX_AMOUNT_IN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(180_000_000_000); // 1800 XLM

        let sample_count = std::env::var("ARB_SAMPLE_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);

        let submit_tx = std::env::var("ARB_SUBMIT_TX")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let dry_run = std::env::var("ARB_DRY_RUN")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(!submit_tx);

        let submit_dedup_secs = std::env::var("ARB_SUBMIT_DEDUP_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);

        Ok(Self {
            snapshot_redis_url,
            pool_state_redis_url,
            aggregator_contract,
            vault_contract,
            caller_secrets,
            caller_secrets_file,
            mnemonic_path,
            caller_indices,
            rpc_url,
            horizon_url,
            base_tokens,
            bridge_tokens,
            probe_amount_in,
            min_profit,
            slippage_bps,
            max_hops,
            max_splits,
            scan_interval_ms,
            build_tx,
            optimize_amount,
            min_amount_in,
            max_amount_in,
            sample_count,
            submit_tx,
            dry_run,
            submit_dedup_secs,
        })
    }
}

fn parse_token_list(raw: &str) -> Vec<TokenId> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(TokenId::from_str_auto)
        .collect()
}

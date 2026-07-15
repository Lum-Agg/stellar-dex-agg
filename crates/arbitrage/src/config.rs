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
    /// LumAgg quote-api base URLs (round-robin). Same stack as
    /// `lumagg-api@{3100..3103}`.
    pub quote_api_urls: Vec<String>,
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
    pub base_tokens: Vec<TokenId>,
    pub bridge_tokens: Vec<TokenId>,
    pub probe_amount_in: u128,
    /// Minimum round-trip profit in base-token stroops (7 decimals for XLM).
    pub min_profit: u128,
    pub slippage_bps: u32,
    pub max_hops: usize,
    pub max_splits: usize,
    /// Delay between full base×bridge cycles (milliseconds). 0 = no pause.
    pub scan_interval_ms: u64,
    /// Gap between yielding consecutive bridge scan items (milliseconds).
    pub item_gap_ms: u64,
    /// Parallel quote/sim workers (stellar-arb style).
    pub worker_count: usize,
    pub build_tx: bool,
    pub optimize_amount: bool,
    pub min_amount_in: u128,
    pub max_amount_in: u128,
    pub sample_count: usize,
    pub submit_tx: bool,
    pub dry_run: bool,
    /// Poll `get_transaction` after submit (Telegram stats). Default off.
    pub poll_tx: bool,
    pub submit_dedup_secs: u64,
}

impl ArbConfig {
    pub fn from_env() -> Result<Self> {
        let quote_api_urls = parse_quote_api_urls_from_env();

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
            .unwrap_or(120_000); // 0.012 XLM net after estimated fees

        let slippage_bps = std::env::var("ARB_SLIPPAGE_BPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let max_hops = std::env::var("ARB_MAX_HOPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        let max_splits = std::env::var("ARB_MAX_SPLITS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

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

        let item_gap_ms = std::env::var("ARB_ITEM_GAP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let worker_count = std::env::var("ARB_WORKER_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4)
            .max(1);

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
            .unwrap_or(18_000_000_000); // 1800 XLM default

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

        let poll_tx = std::env::var("ARB_POLL_TX")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let submit_dedup_secs = std::env::var("ARB_SUBMIT_DEDUP_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);

        Ok(Self {
            quote_api_urls,
            aggregator_contract,
            vault_contract,
            caller_secrets,
            caller_secrets_file,
            mnemonic_path,
            caller_indices,
            rpc_url,
            base_tokens,
            bridge_tokens,
            probe_amount_in,
            min_profit,
            slippage_bps,
            max_hops,
            max_splits,
            scan_interval_ms,
            item_gap_ms,
            worker_count,
            build_tx,
            optimize_amount,
            min_amount_in,
            max_amount_in,
            sample_count,
            submit_tx,
            dry_run,
            poll_tx,
            submit_dedup_secs,
        })
    }
}

const DEFAULT_QUOTE_API_PORTS: &[u16] = &[3100, 3101, 3102, 3103];

fn parse_quote_api_urls_from_env() -> Vec<String> {
    if let Ok(raw) = std::env::var("ARB_QUOTE_API_URLS") {
        let urls = parse_url_list(&raw);
        if !urls.is_empty() {
            return urls;
        }
    }
    for key in ["ARB_QUOTE_API_URL", "QUOTE_API_URL", "LUMAGG_API_URL"] {
        if let Ok(raw) = std::env::var(key) {
            if raw.contains(',') {
                let urls = parse_url_list(&raw);
                if !urls.is_empty() {
                    return urls;
                }
            }
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return vec![trimmed.trim_end_matches('/').to_string()];
            }
        }
    }
    DEFAULT_QUOTE_API_PORTS
        .iter()
        .map(|port| format!("http://127.0.0.1:{port}"))
        .collect()
}

fn parse_url_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .collect()
}

fn parse_token_list(raw: &str) -> Vec<TokenId> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(TokenId::from_str_auto)
        .collect()
}

use anyhow::{Context, Result};

/// Default mainnet aggregator contract (same as api-server).
pub const DEFAULT_AGGREGATOR_CONTRACT: &str = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";

/// Approximate ledgers in one day (~5s close time).
pub const DEFAULT_LOOKBACK_LEDGERS: u32 = 17_280;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexMode {
    Events,
    Envelope,
    Both,
}

impl std::fmt::Display for IndexMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexMode::Events => write!(f, "events"),
            IndexMode::Envelope => write!(f, "envelope"),
            IndexMode::Both => write!(f, "both"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub aggregator_contract: String,
    /// When set, ingest order-escrow lifecycle events into `limit_orders`.
    pub escrow_contract: Option<String>,
    pub index_mode: IndexMode,
    /// When true and mode includes events, also ingest legacy envelope invokes
    /// (pre-upgrade txs).
    pub envelope_fallback: bool,
    pub db_path: String,
    pub poll_secs: u64,
    pub page_limit: u32,
    pub start_ledger: Option<u32>,
}

impl IndexerConfig {
    pub fn from_env() -> Result<Self> {
        let index_mode = match std::env::var("INDEXER_MODE")
            .unwrap_or_else(|_| "events".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "envelope" => IndexMode::Envelope,
            "both" => IndexMode::Both,
            _ => IndexMode::Events,
        };

        let envelope_fallback = std::env::var("INDEXER_ENVELOPE_FALLBACK")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(index_mode != IndexMode::Envelope);

        Ok(Self {
            rpc_url: std::env::var("INDEXER_RPC_URL")
                .or_else(|_| std::env::var("SOROBAN_RPC_URL"))
                .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".into()),
            network_passphrase: std::env::var("INDEXER_NETWORK_PASSPHRASE")
                .unwrap_or_else(|_| "Public Global Stellar Network ; September 2015".into()),
            aggregator_contract: std::env::var("AGGREGATOR_CONTRACT")
                .unwrap_or_else(|_| DEFAULT_AGGREGATOR_CONTRACT.into()),
            escrow_contract: std::env::var("ESCROW_CONTRACT")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            index_mode,
            envelope_fallback,
            db_path: std::env::var("INDEXER_DB_PATH").unwrap_or_else(|_| "./data/analytics-indexer.db".into()),
            poll_secs: std::env::var("INDEXER_POLL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            page_limit: std::env::var("INDEXER_PAGE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(dex_adapters::rpc::events::DEFAULT_EVENTS_PAGE_LIMIT),
            start_ledger: std::env::var("INDEXER_START_LEDGER").ok().and_then(|s| s.parse().ok()),
        })
    }

    pub fn use_events(&self) -> bool {
        matches!(self.index_mode, IndexMode::Events | IndexMode::Both)
    }

    pub fn rpc(&self) -> dex_adapters::SorobanRpc {
        dex_adapters::SorobanRpc::new(&self.rpc_url, &self.network_passphrase)
    }

    pub fn ensure_parent_dir(&self) -> Result<()> {
        if let Some(parent) = std::path::Path::new(&self.db_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create db parent dir {}", parent.display()))?;
            }
        }
        Ok(())
    }
}

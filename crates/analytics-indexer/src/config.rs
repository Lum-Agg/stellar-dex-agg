use {
    anyhow::{Context, Result},
    lumagg_config::aggregator::AggregatorConfig,
};

/// Mainnet contract used by the standalone fixture-fetching utility.
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
    pub horizon_url: Option<String>,
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
    pub fn from_aggregator(config: &AggregatorConfig) -> Result<Self> {
        config.validate_indexer()?;
        let indexer = config.indexer.as_ref().context("indexer section is required")?;
        let index_mode = match indexer.mode.as_str() {
            "envelope" => IndexMode::Envelope,
            "both" => IndexMode::Both,
            "events" => IndexMode::Events,
            mode => anyhow::bail!("unsupported indexer mode: {mode}"),
        };

        Ok(Self {
            rpc_url: config.network.rpc_url.clone(),
            horizon_url: config.dex.horizon_url.clone(),
            network_passphrase: config.network.passphrase.clone(),
            aggregator_contract: config
                .api
                .aggregator_contract
                .clone()
                .context("api.aggregator_contract is required for the indexer")?,
            escrow_contract: config.features.escrow_contract.clone(),
            index_mode,
            envelope_fallback: indexer.envelope_fallback,
            db_path: indexer.db_path.clone(),
            poll_secs: indexer.poll_secs,
            page_limit: indexer.page_limit,
            start_ledger: indexer.start_ledger,
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

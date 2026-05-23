use anyhow::Result;
use dex_adapters::{
    aquarius::AquariusAdapter,
    aquarius_clmm::AquariusClmmAdapter,
    classic_dex::ClassicDexAdapter,
    comet::CometAdapter,
    phoenix::PhoenixAdapter,
    rpc::SorobanRpc,
    soroswap::SoroswapAdapter,
    sushi::SushiAdapter,
    traits::AdapterTradingPair,
    token_metadata::{TokenMetadata, TokenMetadataStore},
    DexAdapter,
};
use market_snapshot::{
    pool_state_store::build_pool_state_store,
    store::{
        build_snapshot_store, DEFAULT_REDIS_EVENTS_CHANNEL, DEFAULT_REDIS_SNAPSHOT_HISTORY,
        SnapshotStoreBackend,
    },
    ClmmPoolSnapshot, MarketSnapshot, SourceSnapshot, TokenMetadataSnapshot, TradingPairSnapshot,
    DEFAULT_SNAPSHOT_DIR,
};
use std::{path::PathBuf, sync::Arc};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub snapshot_backend: SnapshotStoreBackend,
    pub snapshot_dir: PathBuf,
    pub snapshot_redis_url: Option<String>,
    pub snapshot_redis_channel: String,
    pub snapshot_redis_keep_latest: usize,
    pub refresh_interval_secs: u64,
    pub discovery_interval_secs: u64,
    pub ledger_poll: std::time::Duration,
    pub ledger_watcher_enabled: bool,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let snapshot_backend = infer_snapshot_backend(
            std::env::var("SNAPSHOT_BACKEND").ok().as_deref(),
            std::env::var("SNAPSHOT_REDIS_URL").ok().as_deref(),
        )?;
        Ok(Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string()),
            network_passphrase: std::env::var("NETWORK_PASSPHRASE")
                .unwrap_or_else(|_| "Public Global Stellar Network ; September 2015".to_string()),
            snapshot_backend,
            snapshot_dir: std::env::var("SNAPSHOT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(DEFAULT_SNAPSHOT_DIR)),
            snapshot_redis_url: std::env::var("SNAPSHOT_REDIS_URL").ok(),
            snapshot_redis_channel: std::env::var("SNAPSHOT_REDIS_CHANNEL")
                .unwrap_or_else(|_| DEFAULT_REDIS_EVENTS_CHANNEL.to_string()),
            snapshot_redis_keep_latest: std::env::var("SNAPSHOT_REDIS_KEEP_LATEST")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_REDIS_SNAPSHOT_HISTORY),
            refresh_interval_secs: std::env::var("REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5),
            discovery_interval_secs: std::env::var("DISCOVERY_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(600),
            ledger_poll: crate::ledger_watcher::ledger_poll_duration_from_env(),
            ledger_watcher_enabled: crate::ledger_watcher::ledger_watcher_enabled_from_env(),
        })
    }
}

fn infer_snapshot_backend(
    snapshot_backend: Option<&str>,
    snapshot_redis_url: Option<&str>,
) -> Result<SnapshotStoreBackend> {
    if let Some(backend) = snapshot_backend {
        return SnapshotStoreBackend::parse(backend);
    }
    if snapshot_redis_url.is_some() {
        return Ok(SnapshotStoreBackend::Redis);
    }
    Ok(SnapshotStoreBackend::File)
}

fn trading_pair_snapshot(pair: &AdapterTradingPair) -> TradingPairSnapshot {
    TradingPairSnapshot {
        token_a: pair.token_a.canonical(),
        token_b: pair.token_b.canonical(),
        pool_address: pair.pool_address.clone(),
        fee_bps: pair.fee_bps,
    }
}

fn sanitize_source_pairs(source: &str, pairs: Vec<TradingPairSnapshot>) -> Vec<TradingPairSnapshot> {
    if source != "aquarius" {
        return pairs;
    }

    let mut by_pool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pair in &pairs {
        *by_pool.entry(pair.pool_address.clone()).or_insert(0) += 1;
    }

    pairs
        .into_iter()
        .filter(|pair| by_pool.get(&pair.pool_address).copied().unwrap_or(0) == 1)
        .collect()
}

async fn collect_sources_from_discovery(adapters: &[Arc<dyn DexAdapter>]) -> Vec<SourceSnapshot> {
    let mut sources = Vec::new();
    for adapter in adapters {
        match adapter.get_trading_pairs().await {
            Ok(pairs) => {
                let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
                let pairs = sanitize_source_pairs(adapter.id(), pairs);
                sources.push(SourceSnapshot {
                    source: adapter.id().to_string(),
                    pairs,
                });
            }
            Err(error) => {
                warn!("Discovery failed for {}: {}", adapter.id(), error);
            }
        }
    }
    sources
}

async fn snapshot_from_sources(
    sources: Vec<SourceSnapshot>,
    clmm_pool_refs: Vec<market_snapshot::ClmmPoolRefSnapshot>,
    network_passphrase: &str,
    token_metadata: &TokenMetadataStore,
) -> Result<MarketSnapshot> {
    let generated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let snapshot = MarketSnapshot::from_sources(
        format!("snapshot-{}", generated_at_ms),
        generated_at_ms,
        network_passphrase,
        sources,
    );
    let token_addresses = snapshot.token_addresses().into_iter().collect::<Vec<_>>();
    token_metadata.resolve_unknown(token_addresses.clone()).await;
    let metadata = token_metadata.get_all().await;
    let token_metadata = token_addresses
        .into_iter()
        .filter_map(|address| metadata.get(&address).cloned())
        .map(token_metadata_snapshot)
        .collect::<Vec<_>>();

    Ok(snapshot
        .with_token_metadata(token_metadata)
        .with_clmm_pool_refs(clmm_pool_refs))
}

fn upsert_source_snapshot(
    mut current_sources: Vec<SourceSnapshot>,
    updated_source: SourceSnapshot,
) -> Vec<SourceSnapshot> {
    if let Some(existing) = current_sources
        .iter_mut()
        .find(|source| source.source == updated_source.source)
    {
        *existing = updated_source;
    } else {
        current_sources.push(updated_source);
    }
    current_sources
}

async fn refresh_sources(
    adapters: &[Arc<dyn DexAdapter>],
    current_sources: Vec<SourceSnapshot>,
) -> Vec<SourceSnapshot> {
    let mut sources = current_sources;
    for adapter in adapters {
        match adapter.refresh_reserves().await {
            Ok(updated) if updated > 0 => {
                let pairs = adapter.get_cached_pairs().await;
                if pairs.is_empty() {
                    continue;
                }
                let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
                let pairs = sanitize_source_pairs(adapter.id(), pairs);
                sources = upsert_source_snapshot(
                    sources,
                    SourceSnapshot {
                        source: adapter.id().to_string(),
                        pairs,
                    },
                );
            }
            Ok(_) => {}
            Err(error) => warn!("Reserve refresh failed for {}: {}", adapter.id(), error),
        }
    }
    sources
}

fn token_metadata_snapshot(meta: TokenMetadata) -> TokenMetadataSnapshot {
    TokenMetadataSnapshot {
        contract: meta.contract,
        symbol: meta.symbol,
        name: meta.name,
        logo: meta.logo,
    }
}

async fn collect_clmm_snapshots(
    sushi: &SushiAdapter,
    aquarius_clmm: &AquariusClmmAdapter,
) -> Vec<ClmmPoolSnapshot> {
    let mut clmm_pools = sushi.export_clmm_snapshots().await;
    clmm_pools.extend(aquarius_clmm.export_clmm_snapshots().await);
    clmm_pools.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.pool_address.cmp(&b.pool_address))
    });
    log_clmm_coverage_stats(&clmm_pools);
    clmm_pools
}

fn log_clmm_coverage_stats(clmm_pools: &[ClmmPoolSnapshot]) {
    let mut complete = 0usize;
    let mut incomplete = 0usize;
    let mut no_coverage = 0usize;
    for pool in clmm_pools {
        match pool.coverage.as_ref() {
            Some(c) if c.is_complete => complete += 1,
            Some(_) => incomplete += 1,
            None => no_coverage += 1,
        }
    }
    info!(
        clmm_pools = clmm_pools.len(),
        complete,
        incomplete,
        no_coverage,
        "CLMM snapshot coverage"
    );
}

async fn publish_snapshot_and_pool_state(
    snapshot_store: &dyn market_snapshot::store::SnapshotStore,
    pool_state_store: Option<&market_snapshot::pool_state_store::RedisPoolStateStore>,
    topology: &MarketSnapshot,
    xyk_values: &[market_snapshot::pool_state_store::XykPoolStateValue],
    clmm_states: &[ClmmPoolSnapshot],
) -> Result<()> {
    if let Some(pool_store) = pool_state_store {
        pool_store
            .publish_pool_state(xyk_values, clmm_states)
            .await?;
    }
    snapshot_store.publish_snapshot(topology).await?;
    Ok(())
}

pub async fn run(config: WorkerConfig) -> Result<()> {
    let snapshot_store = build_snapshot_store(
        config.snapshot_backend,
        Some(config.snapshot_dir.clone()),
        config.snapshot_redis_url.as_deref(),
        Some(config.snapshot_redis_channel.as_str()),
        Some(config.snapshot_redis_keep_latest),
    )?;
    let pool_state_store = config
        .snapshot_redis_url
        .as_deref()
        .map(build_pool_state_store)
        .transpose()?;
    let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
    let token_metadata = TokenMetadataStore::new(rpc.clone());
    let soroswap = Arc::new(SoroswapAdapter::new(rpc.clone()));
    let aquarius = Arc::new(AquariusAdapter::new(rpc.clone()));
    let phoenix = Arc::new(PhoenixAdapter::new(rpc.clone()));
    let sushi = Arc::new(SushiAdapter::new(rpc.clone()));
    let comet = Arc::new(CometAdapter::new(rpc.clone()));
    let classic = Arc::new(ClassicDexAdapter::new(None));
    let aquarius_clmm = Arc::new(AquariusClmmAdapter::new(rpc.clone()));
    let adapters: Vec<Arc<dyn DexAdapter>> = vec![
        soroswap.clone(),
        aquarius.clone(),
        phoenix.clone(),
        sushi.clone(),
        comet.clone(),
        classic.clone(),
        aquarius_clmm.clone(),
    ];

    let mut discovery_interval = tokio::time::interval(std::time::Duration::from_secs(
        config.discovery_interval_secs,
    ));
    discovery_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    discovery_interval.tick().await;
    let mut refresh_interval =
        tokio::time::interval(std::time::Duration::from_secs(config.refresh_interval_secs));
    refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    refresh_interval.tick().await;

    let ledger_watcher_enabled =
        config.ledger_watcher_enabled && pool_state_store.is_some();
    let mut ledger_watcher = if ledger_watcher_enabled {
        let mut watcher = crate::ledger_watcher::LedgerWatcher::new(SorobanRpc::new(
            &config.rpc_url,
            &config.network_passphrase,
        ));
        watcher.bootstrap().await?;
        Some(watcher)
    } else {
        None
    };
    let mut ledger_interval = tokio::time::interval(config.ledger_poll);
    ledger_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ledger_interval.tick().await;

    let mut current_sources = collect_sources_from_discovery(&adapters).await;
    let mut current_clmm_pools = collect_clmm_snapshots(&sushi, &aquarius_clmm).await;
    let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&current_clmm_pools);
    let snapshot = snapshot_from_sources(
        current_sources.clone(),
        clmm_refs,
        &config.network_passphrase,
        &token_metadata,
    )
    .await?;
    let xyk_values = crate::pool_state_publish::collect_xyk_pool_state(&adapters).await;
    publish_snapshot_and_pool_state(
        snapshot_store.as_ref(),
        pool_state_store.as_ref(),
        &snapshot,
        &xyk_values,
        &current_clmm_pools,
    )
    .await?;
    info!(
        "Published snapshot {} with {} sources to {}",
        snapshot.version,
        snapshot.sources.len(),
        snapshot_destination(&config)
    );

    loop {
        tokio::select! {
            _ = discovery_interval.tick() => {
                current_sources = collect_sources_from_discovery(&adapters).await;
                current_clmm_pools = collect_clmm_snapshots(&sushi, &aquarius_clmm).await;
            }
            _ = refresh_interval.tick() => {
                // Fast path: xy=k reserves + CLMM slot0; adapters rescan ticks when price drifts.
                current_sources = refresh_sources(&adapters, current_sources).await;
                current_clmm_pools = collect_clmm_snapshots(&sushi, &aquarius_clmm).await;
            }
            _ = ledger_interval.tick(), if ledger_watcher.is_some() && pool_state_store.is_some() => {
                let watcher = ledger_watcher.as_mut().unwrap();
                let store = pool_state_store.as_ref().unwrap();
                let index = crate::ledger_watcher::rebuild_pool_index(
                    &current_sources,
                    &current_clmm_pools,
                );
                match watcher.poll_touched_pools(&index).await {
                    Ok(touched) if !touched.is_empty() => {
                        let rpc = SorobanRpc::new(&config.rpc_url, &config.network_passphrase);
                        match crate::touched_refresh::refresh_touched_pools(
                            crate::touched_refresh::TouchedRefreshContext {
                                rpc: &rpc,
                                pool_store: store,
                                _soroswap: &soroswap,
                                _aquarius: &aquarius,
                                phoenix: &phoenix,
                                comet: &comet,
                                sushi: &sushi,
                                aquarius_clmm: &aquarius_clmm,
                                sources: &mut current_sources,
                                clmm_pools: &mut current_clmm_pools,
                            },
                            touched,
                        )
                        .await
                        {
                            Ok(n) => info!(updated = n, "Ledger-touched pool refresh"),
                            Err(error) => warn!("Ledger-touched pool refresh failed: {}", error),
                        }
                    }
                    Ok(_) => {}
                    Err(error) => warn!("Ledger watcher poll failed: {}", error),
                }
            }
        }

        let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&current_clmm_pools);
        let snapshot = snapshot_from_sources(
            current_sources.clone(),
            clmm_refs,
            &config.network_passphrase,
            &token_metadata,
        )
        .await?;
        let xyk_values = crate::pool_state_publish::collect_xyk_pool_state(&adapters).await;
        publish_snapshot_and_pool_state(
            snapshot_store.as_ref(),
            pool_state_store.as_ref(),
            &snapshot,
            &xyk_values,
            &current_clmm_pools,
        )
        .await?;
        info!(
            "Published snapshot {} with {} sources to {}",
            snapshot.version,
            snapshot.sources.len(),
            snapshot_destination(&config)
        );
    }
}

fn snapshot_destination(config: &WorkerConfig) -> String {
    match config.snapshot_backend {
        SnapshotStoreBackend::File => config.snapshot_dir.display().to_string(),
        SnapshotStoreBackend::Redis => config
            .snapshot_redis_url
            .clone()
            .unwrap_or_else(|| "redis".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use market_snapshot::ClmmPoolSnapshot;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_clmm_pool() -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "pool-clmm".to_string(),
            token0: "A".to_string(),
            token1: "B".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [1, 2, 3, 4],
            tick: 120,
            liquidity: 10_000,
            ticks: Vec::new(),
            chunk_bitmaps: Vec::new(),
            word_bitmaps: Vec::new(),
            coverage: None,
        }
    }

    #[test]
    fn sanitizes_aquarius_multi_edge_pools() {
        let filtered = sanitize_source_pairs(
            "aquarius",
            vec![
                TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                },
                TradingPairSnapshot {
                    token_a: "B".to_string(),
                    token_b: "C".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                },
                TradingPairSnapshot {
                    token_a: "X".to_string(),
                    token_b: "Y".to_string(),
                    pool_address: "pool-2".to_string(),
                    fee_bps: 30,
                },
            ],
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pool_address, "pool-2");
    }

    #[test]
    fn upserts_source_snapshot_without_dropping_others() {
        let current = vec![
            SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "old".to_string(),
                    fee_bps: 30,
                }],
            },
            SourceSnapshot {
                source: "phoenix".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "C".to_string(),
                    token_b: "D".to_string(),
                    pool_address: "keep".to_string(),
                    fee_bps: 30,
                }],
            },
        ];

        let updated = upsert_source_snapshot(
            current,
            SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "new".to_string(),
                    fee_bps: 30,
                }],
            },
        );

        assert_eq!(updated.len(), 2);
        assert!(updated.iter().any(|source| source.source == "phoenix"));
        assert_eq!(
            updated
                .iter()
                .find(|source| source.source == "soroswap")
                .unwrap()
                .pairs[0]
                .pool_address,
            "new"
        );
    }

    #[test]
    fn worker_config_reads_snapshot_redis_channel_and_keep_latest() {
        let _guard = env_lock().lock().unwrap();
        let original_channel = std::env::var("SNAPSHOT_REDIS_CHANNEL").ok();
        let original_keep_latest = std::env::var("SNAPSHOT_REDIS_KEEP_LATEST").ok();
        std::env::set_var("SNAPSHOT_REDIS_CHANNEL", "snapshots:worker");
        std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", "24");

        let config = WorkerConfig::from_env().unwrap();

        assert_eq!(config.snapshot_redis_channel, "snapshots:worker");
        assert_eq!(config.snapshot_redis_keep_latest, 24);

        match original_channel {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_CHANNEL", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_CHANNEL"),
        }
        match original_keep_latest {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_KEEP_LATEST"),
        }
    }

    #[test]
    fn infers_redis_backend_when_only_redis_url_is_set() {
        assert_eq!(
            infer_snapshot_backend(None, Some("redis://127.0.0.1/")).unwrap(),
            SnapshotStoreBackend::Redis
        );
        assert_eq!(infer_snapshot_backend(None, None).unwrap(), SnapshotStoreBackend::File);
    }

    #[tokio::test]
    async fn snapshot_from_sources_preserves_clmm_pool_refs() {
        let rpc = Arc::new(SorobanRpc::new(
            "https://soroban-rpc.mainnet.stellar.gateway.fm",
            "Public Global Stellar Network ; September 2015",
        ));
        let token_metadata = TokenMetadataStore::new(rpc);
        token_metadata
            .replace_all(std::collections::HashMap::from([
                (
                    "A".to_string(),
                    TokenMetadata {
                        contract: "A".to_string(),
                        symbol: "TOKA".to_string(),
                        name: "Token A".to_string(),
                        logo: None,
                    },
                ),
                (
                    "B".to_string(),
                    TokenMetadata {
                        contract: "B".to_string(),
                        symbol: "TOKB".to_string(),
                        name: "Token B".to_string(),
                        logo: None,
                    },
                ),
            ]))
            .await;

        let snapshot = snapshot_from_sources(
            vec![SourceSnapshot {
                source: "sushi".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool-clmm".to_string(),
                    fee_bps: 30,
                }],
            }],
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())],
            "mainnet",
            &token_metadata,
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot.clmm_pool_refs,
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())]
        );
        assert_eq!(snapshot.token_metadata.len(), 2);
    }
}

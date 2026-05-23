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
    token_metadata::{TokenMetadata, TokenMetadataStore},
    traits::AdapterTradingPair,
    DexAdapter,
};
use market_snapshot::{
    pool_state_store::build_pool_state_store,
    store::{
        build_snapshot_store, SnapshotStore, SnapshotStoreBackend, DEFAULT_REDIS_EVENTS_CHANNEL,
        DEFAULT_REDIS_SNAPSHOT_HISTORY,
    },
    ClmmPoolSnapshot, MarketSnapshot, SourceSnapshot, TokenMetadataSnapshot, TradingPairSnapshot,
    DEFAULT_SNAPSHOT_DIR,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Shared graph + CLMM state (main loop and background bootstrap).
pub(crate) struct WorkerShared {
    pub(crate) sources: Vec<SourceSnapshot>,
    pub(crate) clmm_pools: Vec<ClmmPoolSnapshot>,
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub snapshot_backend: SnapshotStoreBackend,
    pub snapshot_dir: PathBuf,
    pub snapshot_redis_url: Option<String>,
    pub snapshot_redis_channel: String,
    pub snapshot_redis_keep_latest: usize,
    /// Heavy adapter.refresh_reserves() cadence (Aquarius batch can take 15–30s).
    pub refresh_interval_secs: u64,
    /// Fast Redis pool-state publish from adapter caches (independent of refresh duration).
    pub pool_publish_interval_secs: u64,
    /// Concurrent getLedgerEntries batches during xy=k refresh (Soroswap/Aquarius).
    pub pool_state_refresh_concurrency: usize,
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
                .unwrap_or(30),
            pool_publish_interval_secs: std::env::var("POOL_PUBLISH_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
            pool_state_refresh_concurrency: std::env::var("POOL_STATE_REFRESH_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4),
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

fn sanitize_source_pairs(
    source: &str,
    pairs: Vec<TradingPairSnapshot>,
) -> Vec<TradingPairSnapshot> {
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

async fn discover_adapter_source(adapter: Arc<dyn DexAdapter>) -> Option<SourceSnapshot> {
    match adapter.get_trading_pairs().await {
        Ok(pairs) => {
            let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
            let pairs = sanitize_source_pairs(adapter.id(), pairs);
            Some(SourceSnapshot {
                source: adapter.id().to_string(),
                pairs,
            })
        }
        Err(error) => {
            warn!("Discovery failed for {}: {}", adapter.id(), error);
            None
        }
    }
}

/// Run adapter discovery concurrently (Aquarius + Soroswap no longer block each other).
async fn collect_sources_from_discovery(adapters: &[Arc<dyn DexAdapter>]) -> Vec<SourceSnapshot> {
    let tasks = adapters.iter().cloned().map(discover_adapter_source);
    futures::future::join_all(tasks)
        .await
        .into_iter()
        .flatten()
        .collect()
}

fn build_topology_snapshot(
    sources: Vec<SourceSnapshot>,
    clmm_pool_refs: Vec<market_snapshot::ClmmPoolRefSnapshot>,
    network_passphrase: &str,
    token_metadata: Vec<TokenMetadataSnapshot>,
) -> MarketSnapshot {
    let generated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    MarketSnapshot::from_sources(
        format!("snapshot-{}", generated_at_ms),
        generated_at_ms,
        network_passphrase,
        sources,
    )
    .with_token_metadata(token_metadata)
    .with_clmm_pool_refs(clmm_pool_refs)
}

/// Resolve token symbols off the hot path, then republish snapshot.
fn spawn_token_metadata_enrichment(
    snapshot_store: Arc<dyn market_snapshot::store::SnapshotStore>,
    token_metadata: Arc<TokenMetadataStore>,
    mut snapshot: MarketSnapshot,
) {
    tokio::spawn(async move {
        let token_addresses: Vec<String> = snapshot.token_addresses().into_iter().collect();
        if token_addresses.is_empty() {
            return;
        }
        token_metadata
            .resolve_unknown(token_addresses.clone())
            .await;
        let metadata = token_metadata.get_all().await;
        let enriched: Vec<TokenMetadataSnapshot> = token_addresses
            .into_iter()
            .filter_map(|address| metadata.get(&address).cloned())
            .map(token_metadata_snapshot)
            .collect();
        snapshot = snapshot.with_token_metadata(enriched);
        match snapshot_store.publish_snapshot(&snapshot).await {
            Ok(()) => info!(
                version = %snapshot.version,
                tokens = snapshot.token_metadata.len(),
                "Republished snapshot after token metadata enrichment"
            ),
            Err(error) => warn!("Token metadata republish failed: {}", error),
        }
    });
}

async fn snapshot_from_sources(
    sources: Vec<SourceSnapshot>,
    clmm_pool_refs: Vec<market_snapshot::ClmmPoolRefSnapshot>,
    network_passphrase: &str,
    existing_token_metadata: Vec<TokenMetadataSnapshot>,
) -> Result<MarketSnapshot> {
    Ok(build_topology_snapshot(
        sources,
        clmm_pool_refs,
        network_passphrase,
        existing_token_metadata,
    ))
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
    refresh_sources_parallel(adapters, current_sources).await
}

/// Refresh every adapter concurrently (each may batch RPC internally).
async fn refresh_sources_parallel(
    adapters: &[Arc<dyn DexAdapter>],
    mut sources: Vec<SourceSnapshot>,
) -> Vec<SourceSnapshot> {
    let snapshots = futures::future::join_all(adapters.iter().map(|adapter| async move {
        let source_id = adapter.id().to_string();
        match adapter.refresh_reserves().await {
            Ok(updated) if updated > 0 => {
                let pairs = adapter.get_cached_pairs().await;
                if pairs.is_empty() {
                    return None;
                }
                let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
                let pairs = sanitize_source_pairs(&source_id, pairs);
                Some(SourceSnapshot {
                    source: source_id,
                    pairs,
                })
            }
            Ok(_) => None,
            Err(error) => {
                warn!("Reserve refresh failed for {}: {}", source_id, error);
                None
            }
        }
    }))
    .await;

    for snapshot in snapshots.into_iter().flatten() {
        sources = upsert_source_snapshot(sources, snapshot);
    }
    sources
}

struct PoolRefreshInFlight(AtomicBool);

impl PoolRefreshInFlight {
    fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn try_start(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    fn finish(&self) {
        self.0.store(false, Ordering::Release);
    }
}

fn spawn_parallel_pool_state_refresh(
    in_flight: Arc<PoolRefreshInFlight>,
    shared: Arc<RwLock<WorkerShared>>,
    adapters: Vec<Arc<dyn DexAdapter>>,
    sushi: Arc<SushiAdapter>,
    aquarius_clmm: Arc<AquariusClmmAdapter>,
    pool_state_store: Option<Arc<market_snapshot::pool_state_store::RedisPoolStateStore>>,
    metrics: Option<Arc<crate::monitor::WorkerMonitorMetrics>>,
    telegram: Option<Arc<lumagg_alerts::TelegramAlerter>>,
) {
    if !in_flight.try_start() {
        debug!("pool state refresh skipped (previous cycle still running)");
        return;
    }

    tokio::spawn(async move {
        struct Guard(Arc<PoolRefreshInFlight>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.finish();
            }
        }
        let _guard = Guard(in_flight);

        let sources = {
            let guard = shared.read().await;
            guard.sources.clone()
        };
        let refreshed = refresh_sources_parallel(&adapters, sources).await;
        let clmm_pools = collect_clmm_snapshots(sushi.as_ref(), aquarius_clmm.as_ref()).await;
        {
            let mut guard = shared.write().await;
            guard.sources = refreshed;
            guard.clmm_pools = clmm_pools.clone();
        }
        if let Err(error) = publish_pool_state_only(
            pool_state_store.as_ref(),
            &adapters,
            &clmm_pools,
            metrics.as_ref(),
        )
        .await
        {
            warn!("pool state Redis publish failed: {}", error);
            crate::monitor::alert_failure(
                telegram.as_ref(),
                "pool_publish_failed",
                &format!("Redis publish failed: {error}"),
            )
            .await;
        }
    });
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
    let (sushi_pools, aquarius_pools) = tokio::join!(
        sushi.export_clmm_snapshots(),
        aquarius_clmm.export_clmm_snapshots(),
    );
    let mut clmm_pools = sushi_pools;
    clmm_pools.extend(aquarius_pools);
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
        complete, incomplete, no_coverage, "CLMM snapshot coverage"
    );
}

async fn publish_pool_state_only(
    pool_state_store: Option<&Arc<market_snapshot::pool_state_store::RedisPoolStateStore>>,
    adapters: &[Arc<dyn DexAdapter>],
    clmm_states: &[ClmmPoolSnapshot],
    metrics: Option<&Arc<crate::monitor::WorkerMonitorMetrics>>,
) -> Result<()> {
    let Some(pool_store) = pool_state_store.map(|s| s.as_ref()) else {
        return Ok(());
    };
    let xyk_values = crate::pool_state_publish::collect_xyk_pool_state(adapters).await;
    let clmm_complete = clmm_states
        .iter()
        .filter(|p| market_snapshot::pool_state_store::should_publish_clmm_to_redis(p))
        .count();
    pool_store
        .publish_pool_state(&xyk_values, clmm_states)
        .await?;
    if let Some(m) = metrics {
        m.record_publish(xyk_values.len(), clmm_complete);
    }
    info!(
        xyk_pools = xyk_values.len(),
        clmm_pools = clmm_complete,
        ttl_secs = pool_store.ttl_secs(),
        "Published pool state to Redis"
    );
    Ok(())
}

async fn publish_snapshot_and_pool_state(
    snapshot_store: &dyn market_snapshot::store::SnapshotStore,
    pool_state_store: Option<&Arc<market_snapshot::pool_state_store::RedisPoolStateStore>>,
    adapters: &[Arc<dyn DexAdapter>],
    topology: &MarketSnapshot,
    clmm_states: &[ClmmPoolSnapshot],
) -> Result<()> {
    publish_pool_state_only(pool_state_store, adapters, clmm_states, None).await?;
    snapshot_store.publish_snapshot(topology).await?;
    Ok(())
}

enum WorkerTick {
    /// Rediscovery: topology snapshot + pool state.
    Discovery,
    /// Periodic adapter.refresh_reserves() (slow).
    Refresh,
    /// Parallel on-chain refresh + Redis publish (every 1–2s; skips if prior cycle still running).
    PoolPublish,
}

pub async fn run(config: WorkerConfig) -> Result<()> {
    let snapshot_store: Arc<dyn SnapshotStore> = Arc::from(build_snapshot_store(
        config.snapshot_backend,
        Some(config.snapshot_dir.clone()),
        config.snapshot_redis_url.as_deref(),
        Some(config.snapshot_redis_channel.as_str()),
        Some(config.snapshot_redis_keep_latest),
    )?);
    let pool_state_store: Option<Arc<market_snapshot::pool_state_store::RedisPoolStateStore>> =
        config
            .snapshot_redis_url
            .as_deref()
            .map(build_pool_state_store)
            .transpose()?
            .map(Arc::new);
    let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
    let token_metadata = Arc::new(TokenMetadataStore::new(rpc.clone()));
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
    let mut pool_publish_interval = tokio::time::interval(std::time::Duration::from_secs(
        config.pool_publish_interval_secs,
    ));
    pool_publish_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    pool_publish_interval.tick().await;

    let ledger_watcher_enabled = config.ledger_watcher_enabled && pool_state_store.is_some();
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
    let ledger_poll = config.ledger_poll;

    let mut seeded_metadata = Vec::new();
    let shared = Arc::new(RwLock::new(WorkerShared {
        sources: Vec::new(),
        clmm_pools: Vec::new(),
    }));
    if let Ok(existing) = snapshot_store.load_current_snapshot().await {
        let mut guard = shared.write().await;
        guard.sources = existing.sources;
        seeded_metadata = existing.token_metadata;
        info!(
            sources = guard.sources.len(),
            "Seeded worker topology from Redis snapshot (pool publish loop starts immediately)"
        );
    }

    let shared_boot = shared.clone();
    let snapshot_store_boot = snapshot_store.clone();
    let token_metadata_boot = token_metadata.clone();
    let adapters_boot = adapters.clone();
    let sushi_boot = sushi.clone();
    let aquarius_clmm_boot = aquarius_clmm.clone();
    let pool_state_boot = pool_state_store.clone();
    let network_passphrase = config.network_passphrase.clone();
    let destination = snapshot_destination(&config);
    tokio::spawn(async move {
        info!("Background bootstrap: parallel adapter discovery");
        let sources = collect_sources_from_discovery(&adapters_boot).await;
        let clmm_pools = collect_clmm_snapshots(&sushi_boot, &aquarius_clmm_boot).await;
        {
            let mut guard = shared_boot.write().await;
            guard.sources = sources.clone();
            guard.clmm_pools = clmm_pools.clone();
        }
        let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&clmm_pools);
        let snapshot =
            match snapshot_from_sources(sources, clmm_refs, &network_passphrase, seeded_metadata)
                .await
            {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    warn!("Background bootstrap snapshot build failed: {}", error);
                    return;
                }
            };
        if let Err(error) = publish_snapshot_and_pool_state(
            snapshot_store_boot.as_ref(),
            pool_state_boot.as_ref(),
            &adapters_boot,
            &snapshot,
            &clmm_pools,
        )
        .await
        {
            warn!("Background bootstrap publish failed: {}", error);
            return;
        }
        info!(
            "Background bootstrap published snapshot {} with {} sources to {}",
            snapshot.version,
            snapshot.sources.len(),
            destination
        );
        spawn_token_metadata_enrichment(snapshot_store_boot, token_metadata_boot, snapshot);
    });

    if let (Some(mut watcher), Some(pool_store)) = (ledger_watcher, pool_state_store.clone()) {
        let shared_ledger = shared.clone();
        let rpc_url = config.rpc_url.clone();
        let network_passphrase = config.network_passphrase.clone();
        let soroswap_ledger = soroswap.clone();
        let aquarius_ledger = aquarius.clone();
        let phoenix_ledger = phoenix.clone();
        let comet_ledger = comet.clone();
        let sushi_ledger = sushi.clone();
        let aquarius_clmm_ledger = aquarius_clmm.clone();
        tokio::spawn(async move {
            let mut ledger_interval = tokio::time::interval(ledger_poll);
            ledger_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ledger_interval.tick().await;
            loop {
                ledger_interval.tick().await;
                let (index_sources, index_clmm) = {
                    let guard = shared_ledger.read().await;
                    (guard.sources.clone(), guard.clmm_pools.clone())
                };
                let index = crate::ledger_watcher::rebuild_pool_index(&index_sources, &index_clmm);
                match watcher.poll_touched_pools(&index).await {
                    Ok(touched) if !touched.is_empty() => {
                        let rpc = SorobanRpc::new(&rpc_url, &network_passphrase);
                        let mut sources = {
                            let guard = shared_ledger.read().await;
                            guard.sources.clone()
                        };
                        let mut clmm_pools = {
                            let guard = shared_ledger.read().await;
                            guard.clmm_pools.clone()
                        };
                        let refresh_result = crate::touched_refresh::refresh_touched_pools(
                            crate::touched_refresh::TouchedRefreshContext {
                                rpc: &rpc,
                                pool_store: pool_store.as_ref(),
                                _soroswap: &soroswap_ledger,
                                _aquarius: &aquarius_ledger,
                                phoenix: &phoenix_ledger,
                                comet: &comet_ledger,
                                sushi: &sushi_ledger,
                                aquarius_clmm: &aquarius_clmm_ledger,
                                sources: &mut sources,
                                clmm_pools: &mut clmm_pools,
                            },
                            touched,
                        )
                        .await;
                        let mut guard = shared_ledger.write().await;
                        guard.sources = sources;
                        guard.clmm_pools = clmm_pools;
                        match refresh_result {
                            Ok(n) => info!(updated = n, "Ledger-touched pool refresh"),
                            Err(error) => {
                                warn!("Ledger-touched pool refresh failed: {}", error)
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => warn!("Ledger watcher poll failed: {}", error),
                }
            }
        });
    }

    let pool_refresh_in_flight = Arc::new(PoolRefreshInFlight::new());
    let monitor_metrics = Arc::new(crate::monitor::WorkerMonitorMetrics::new());
    let telegram = lumagg_alerts::TelegramAlerter::from_env().map(Arc::new);
    if let Some(ref alerter) = telegram {
        let api_health_url = std::env::var("MONITOR_API_HEALTH_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3100/api/v1/health".to_string());
        crate::monitor::spawn_telegram_monitor(
            alerter.clone(),
            monitor_metrics.clone(),
            shared.clone(),
            pool_state_store.clone(),
            api_health_url,
        );
        info!("Telegram monitoring enabled (heartbeat + alerts)");
        let _ = alerter
            .send("🚀 LumAgg worker started (pool refresh + Telegram monitoring)")
            .await;
    }
    info!(
        pool_publish_interval_secs = config.pool_publish_interval_secs,
        pool_state_refresh_concurrency = config.pool_state_refresh_concurrency,
        "Pool state refresh loop: parallel adapter RPC + Redis publish"
    );

    loop {
        // Never await slow adapter work inside `select!` — it starves the pool publish tick.
        let tick = tokio::select! {
            biased;
            _ = pool_publish_interval.tick() => WorkerTick::PoolPublish,
            _ = refresh_interval.tick() => WorkerTick::Refresh,
            _ = discovery_interval.tick() => WorkerTick::Discovery,
        };

        match tick {
            WorkerTick::PoolPublish => {
                spawn_parallel_pool_state_refresh(
                    pool_refresh_in_flight.clone(),
                    shared.clone(),
                    adapters.clone(),
                    sushi.clone(),
                    aquarius_clmm.clone(),
                    pool_state_store.clone(),
                    Some(monitor_metrics.clone()),
                    telegram.clone(),
                );
            }
            WorkerTick::Refresh => {
                let shared_refresh = shared.clone();
                let adapters_refresh = adapters.clone();
                let sushi_refresh = sushi.clone();
                let aquarius_clmm_refresh = aquarius_clmm.clone();
                tokio::spawn(async move {
                    let sources = {
                        let guard = shared_refresh.read().await;
                        guard.sources.clone()
                    };
                    let refreshed = refresh_sources(&adapters_refresh, sources).await;
                    let clmm_pools =
                        collect_clmm_snapshots(&sushi_refresh, &aquarius_clmm_refresh).await;
                    let mut guard = shared_refresh.write().await;
                    guard.sources = refreshed;
                    guard.clmm_pools = clmm_pools;
                });
            }
            WorkerTick::Discovery => {
                let shared_disc = shared.clone();
                let adapters_disc = adapters.clone();
                let sushi_disc = sushi.clone();
                let aquarius_clmm_disc = aquarius_clmm.clone();
                let snapshot_store_disc = snapshot_store.clone();
                let token_metadata_disc = token_metadata.clone();
                let pool_state_disc = pool_state_store.clone();
                let network_passphrase_disc = config.network_passphrase.clone();
                let destination_disc = snapshot_destination(&config);
                tokio::spawn(async move {
                    let sources = collect_sources_from_discovery(&adapters_disc).await;
                    let clmm_pools = collect_clmm_snapshots(&sushi_disc, &aquarius_clmm_disc).await;
                    {
                        let mut guard = shared_disc.write().await;
                        guard.sources = sources.clone();
                        guard.clmm_pools = clmm_pools.clone();
                    }
                    let metadata_seed = snapshot_store_disc
                        .load_current_snapshot()
                        .await
                        .map(|s| s.token_metadata)
                        .unwrap_or_default();
                    let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&clmm_pools);
                    let snapshot = match snapshot_from_sources(
                        sources,
                        clmm_refs,
                        &network_passphrase_disc,
                        metadata_seed,
                    )
                    .await
                    {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            warn!("Periodic discovery snapshot build failed: {}", error);
                            return;
                        }
                    };
                    if let Err(error) = publish_snapshot_and_pool_state(
                        snapshot_store_disc.as_ref(),
                        pool_state_disc.as_ref(),
                        &adapters_disc,
                        &snapshot,
                        &clmm_pools,
                    )
                    .await
                    {
                        warn!("Periodic discovery publish failed: {}", error);
                        return;
                    }
                    info!(
                        "Published snapshot {} with {} sources to {}",
                        snapshot.version,
                        snapshot.sources.len(),
                        destination_disc
                    );
                    spawn_token_metadata_enrichment(
                        snapshot_store_disc,
                        token_metadata_disc,
                        snapshot,
                    );
                });
            }
        }
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
        assert_eq!(
            infer_snapshot_backend(None, None).unwrap(),
            SnapshotStoreBackend::File
        );
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

        let seeded = vec![
            token_metadata_snapshot(TokenMetadata {
                contract: "A".to_string(),
                symbol: "TOKA".to_string(),
                name: "Token A".to_string(),
                logo: None,
            }),
            token_metadata_snapshot(TokenMetadata {
                contract: "B".to_string(),
                symbol: "TOKB".to_string(),
                name: "Token B".to_string(),
                logo: None,
            }),
        ];
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
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(
                &sample_clmm_pool(),
            )],
            "mainnet",
            seeded,
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot.clmm_pool_refs,
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(
                &sample_clmm_pool()
            )]
        );
        assert_eq!(snapshot.token_metadata.len(), 2);
    }
}

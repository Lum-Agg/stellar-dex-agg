use {
    crate::{
        config::{AppConfig, LumaggMode},
        pool_hydrate::{self, PoolHydrateConfig},
        price_sampler::spawn_price_sampler,
        price_store::PriceStore,
        snapshot_loader::{build_engine_from_snapshot, path_finder_config_from_app},
    },
    anyhow::Result,
    dex_adapters::{
        aquarius::AquariusAdapter,
        aquarius_clmm::AquariusClmmAdapter,
        cache::{default_cache_path, PoolCache},
        classic_dex::ClassicDexAdapter,
        comet::CometAdapter,
        phoenix::PhoenixAdapter,
        rpc::SorobanRpc,
        soroswap::SoroswapAdapter,
        sushi::SushiAdapter,
        token_metadata::{LogoKind, TokenMetadata, TokenMetadataStore},
        traits::AdapterTradingPair,
        DexAdapter,
    },
    market_snapshot::{
        pool_state_store::{build_pool_state_store, MemoryPoolStateStore, PoolStateStore},
        store::{
            build_snapshot_store, should_reload_snapshot_version, subscribe_to_snapshot_events, MemorySnapshotStore,
            SnapshotListenerEvent, SnapshotStore, SnapshotStoreBackend,
        },
        CurrentSnapshotMeta, MarketSnapshot,
    },
    router_engine::{split_optimizer::SplitConfig, OptimalRoute, QuoteEngine, RouteRequest},
    std::{path::PathBuf, sync::Arc},
    tokio::sync::{mpsc, RwLock},
    tracing::{debug, info, warn},
};

/// A quoted route together with the freshness of the hydrated pool state used
/// for that route.
pub struct QuoteRouteResult {
    pub route: OptimalRoute,
    pub oldest_pool_age_ms: Option<u64>,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RwLock<Arc<QuoteEngine>>>,
    pub config: AppConfig,
    pub token_metadata: Arc<TokenMetadataStore>,
    pub rpc: Arc<SorobanRpc>,
    pub pool_state_store: Option<Arc<dyn PoolStateStore>>,
    pub telegram: Option<Arc<lumagg_alerts::TelegramAlerter>>,
    pub price_store: Option<Arc<PriceStore>>,
    pub snapshot_meta: Arc<RwLock<Option<CurrentSnapshotMeta>>>,
}

pub(crate) fn sanitize_cached_pairs(
    source: &str,
    pairs: Vec<router_engine::TradingPair>,
) -> Vec<router_engine::TradingPair> {
    if source != "aquarius" {
        return pairs;
    }

    let mut by_pool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pair in &pairs {
        *by_pool.entry(pair.pool_address.clone()).or_insert(0) += 1;
    }

    // Aquarius multi-token pools are represented as multiple edges sharing one pool
    // address. Those routes are not executable by the current on-chain
    // aggregator, so never hydrate them from disk cache during startup.
    pairs
        .into_iter()
        .filter(|pair| by_pool.get(&pair.pool_address).copied().unwrap_or(0) == 1)
        .collect()
}

fn snapshot_token_metadata(snapshot: &MarketSnapshot) -> std::collections::HashMap<String, TokenMetadata> {
    snapshot
        .token_metadata
        .iter()
        .map(|meta| {
            (
                meta.contract.clone(),
                TokenMetadata {
                    contract: meta.contract.clone(),
                    symbol: meta.symbol.clone(),
                    name: meta.name.clone(),
                    logo: meta.logo.clone(),
                    logo_kind: meta.logo_kind.as_deref().and_then(|k| match k {
                        "official" => Some(LogoKind::Official),
                        "fallback" => Some(LogoKind::Fallback),
                        _ => None,
                    }),
                },
            )
        })
        .collect()
}

fn pairs_to_trading(pairs: &[AdapterTradingPair], source: &str) -> Vec<router_engine::TradingPair> {
    pairs
        .iter()
        .map(|p| router_engine::TradingPair {
            token_a: p.token_a.clone(),
            token_b: p.token_b.clone(),
            source: source.to_string(),
            pool_address: p.pool_address.clone(),
            fee_bps: p.fee_bps,
            reserve_a: p.reserve_a,
            reserve_b: p.reserve_b,
        })
        .collect()
}

fn configured_snapshot_backend(config: &AppConfig) -> Result<Option<SnapshotStoreBackend>> {
    if let Some(backend) = config.snapshot_backend.as_deref() {
        return SnapshotStoreBackend::parse(backend).map(Some);
    }
    if config.snapshot_redis_url.is_some() {
        return Ok(Some(SnapshotStoreBackend::Redis));
    }
    if config.snapshot_dir.is_some() {
        return Ok(Some(SnapshotStoreBackend::File));
    }
    Ok(None)
}

fn configured_pool_state_store(config: &AppConfig) -> Result<Option<Arc<dyn PoolStateStore>>> {
    let Some(redis_url) = config.snapshot_redis_url.as_deref() else {
        return Ok(None);
    };
    Ok(Some(Arc::new(build_pool_state_store(redis_url)?)))
}

fn configured_snapshot_store(config: &AppConfig) -> Result<Option<Arc<dyn SnapshotStore>>> {
    let Some(backend) = configured_snapshot_backend(config)? else {
        return Ok(None);
    };

    Ok(Some(build_snapshot_store(
        backend,
        config.snapshot_dir.clone().map(PathBuf::from),
        config.snapshot_redis_url.as_deref(),
        Some(config.snapshot_redis_channel.as_str()),
        Some(config.snapshot_redis_keep_latest),
    )?))
}

fn configured_snapshot_event_listener(
    config: &AppConfig,
) -> Result<Option<mpsc::UnboundedReceiver<SnapshotListenerEvent>>> {
    let Some(backend) = configured_snapshot_backend(config)? else {
        return Ok(None);
    };

    if backend != SnapshotStoreBackend::Redis {
        return Ok(None);
    }

    let Some(redis_url) = config.snapshot_redis_url.as_deref() else {
        return Ok(None);
    };

    match subscribe_to_snapshot_events(redis_url, &config.snapshot_redis_channel) {
        Ok(listener) => Ok(Some(listener)),
        Err(error) => {
            warn!(
                "Failed to start snapshot pub/sub listener, using polling only: {}",
                error
            );
            Ok(None)
        }
    }
}

fn configured_price_store() -> Result<Option<Arc<PriceStore>>> {
    let Some(path) = std::env::var("PRICE_DB_PATH").ok().filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    Ok(Some(Arc::new(PriceStore::open(path)?)))
}

/// Bridge [`MemorySnapshotStore`] version watch → same event channel as Redis
/// pub/sub.
fn subscribe_memory_snapshot_versions(
    store: Arc<MemorySnapshotStore>,
) -> mpsc::UnboundedReceiver<SnapshotListenerEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut version_rx = store.subscribe_versions();
    tokio::spawn(async move {
        let _ = tx.send(SnapshotListenerEvent::ListenerHealthy);
        loop {
            if version_rx.changed().await.is_err() {
                break;
            }
            let version = version_rx.borrow().clone();
            let Some(version) = version else {
                continue;
            };
            if tx.send(SnapshotListenerEvent::SnapshotVersion(version)).is_err() {
                break;
            }
        }
    });
    rx
}

async fn new_embedded(config: AppConfig) -> Result<AppState> {
    info!("LUMAGG_MODE=embedded: in-process market-data-worker + memory stores (no Redis required)");

    let memory_snapshot = MemorySnapshotStore::shared();
    let memory_pool = MemoryPoolStateStore::shared();
    let snapshot_store: Arc<dyn SnapshotStore> = memory_snapshot.clone();
    let pool_state_store: Option<Arc<dyn PoolStateStore>> = Some(memory_pool.clone());

    let mut worker_cfg = market_data_worker::worker::WorkerConfig::from_env()?;
    worker_cfg.rpc_url = config.rpc_url.clone();
    worker_cfg.network_passphrase = config.network_passphrase.clone();
    worker_cfg.snapshot_backend = SnapshotStoreBackend::Memory;
    worker_cfg.snapshot_redis_url = None;
    worker_cfg.snapshot_store = Some(snapshot_store.clone());
    worker_cfg.pool_store = pool_state_store.clone();

    tokio::spawn(async move {
        let mut retry_secs = 1u64;
        loop {
            match market_data_worker::worker::run(worker_cfg.clone()).await {
                Ok(()) => warn!("embedded market-data-worker stopped unexpectedly"),
                Err(error) => warn!(
                    error = %error,
                    retry_secs,
                    "embedded market-data-worker exited; restarting"
                ),
            }
            tokio::time::sleep(std::time::Duration::from_secs(retry_secs)).await;
            retry_secs = (retry_secs * 2).min(30);
        }
    });

    let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
    let token_metadata = Arc::new(TokenMetadataStore::new(rpc.clone()));
    let snapshot_events = Some(subscribe_memory_snapshot_versions(memory_snapshot));
    let (engine, initial_version, initial_token_metadata, initial_snapshot_meta) =
        load_initial_snapshot_engine(&config, snapshot_store.as_ref()).await?;
    if let Some(token_metadata_map) = initial_token_metadata {
        token_metadata.replace_all(token_metadata_map).await;
    }

    let telegram = lumagg_alerts::TelegramAlerter::from_env_api_primary().map(Arc::new);
    let price_store = configured_price_store()?;
    let state = AppState {
        engine: Arc::new(RwLock::new(engine)),
        config,
        token_metadata,
        rpc,
        pool_state_store,
        telegram,
        price_store,
        snapshot_meta: Arc::new(RwLock::new(initial_snapshot_meta)),
    };
    state.spawn_snapshot_reloader(snapshot_store, snapshot_events, initial_version);
    state.spawn_price_sampler_if_configured();
    Ok(state)
}

fn normalize_snapshot_poll_interval_ms(interval_ms: u64) -> u64 {
    interval_ms.max(1)
}

fn build_empty_quote_engine(config: &AppConfig) -> Arc<QuoteEngine> {
    let split_config = SplitConfig {
        split_threshold_bps: config.split_threshold_bps,
        split_competitive_delta_bps: config.split_competitive_delta_bps,
        min_split_fraction_bps: config.min_split_fraction_bps,
        max_splits: config.max_splits,
        ..SplitConfig::default()
    };
    Arc::new(QuoteEngine::new(path_finder_config_from_app(config), split_config))
}

async fn attach_snapshot_live_adapter(engine: &Arc<QuoteEngine>, adapter: Arc<dyn DexAdapter>) -> Result<()> {
    engine.register_adapter(adapter).await;
    Ok(())
}

async fn attach_snapshot_live_classic_adapter(engine: &Arc<QuoteEngine>) -> Result<()> {
    attach_snapshot_live_adapter(engine, Arc::new(ClassicDexAdapter::new(None))).await
}

async fn load_initial_snapshot_engine(
    config: &AppConfig,
    snapshot_store: &dyn SnapshotStore,
) -> Result<(
    Arc<QuoteEngine>,
    Option<String>,
    Option<std::collections::HashMap<String, TokenMetadata>>,
    Option<CurrentSnapshotMeta>,
)> {
    match snapshot_store.load_current_snapshot().await {
        Ok(snapshot) => {
            let version = snapshot.version.clone();
            let engine = Arc::new(build_engine_from_snapshot(config, &snapshot).await?);
            attach_snapshot_live_classic_adapter(&engine).await?;
            Ok((
                engine,
                Some(version),
                Some(snapshot_token_metadata(&snapshot)),
                Some(snapshot.current_meta()),
            ))
        }
        Err(error) => {
            warn!(
                "Initial snapshot unavailable, starting with empty engine until reload succeeds: {}",
                error
            );
            Ok((build_empty_quote_engine(config), None, None, None))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotReloadMode {
    PollingOnly,
    PollingFallback,
    PubSubHealthy,
}

enum SnapshotReloadTrigger {
    PollTick,
    ListenerEvent(SnapshotListenerEvent),
    ListenerClosed,
}

fn reload_mode_uses_polling(mode: SnapshotReloadMode) -> bool {
    mode != SnapshotReloadMode::PubSubHealthy
}

fn next_snapshot_reload_mode(current_mode: SnapshotReloadMode, event: &SnapshotListenerEvent) -> SnapshotReloadMode {
    match event {
        SnapshotListenerEvent::ListenerHealthy => SnapshotReloadMode::PubSubHealthy,
        SnapshotListenerEvent::ListenerDegraded => match current_mode {
            SnapshotReloadMode::PollingOnly => SnapshotReloadMode::PollingOnly,
            SnapshotReloadMode::PollingFallback | SnapshotReloadMode::PubSubHealthy => {
                SnapshotReloadMode::PollingFallback
            }
        },
        SnapshotListenerEvent::SnapshotVersion(_) => current_mode,
    }
}

/// Full pool discovery for all adapters: replace graph edges per source and
/// persist cache.
async fn run_discovery(adapters: &[Arc<dyn DexAdapter>], engine: &Arc<QuoteEngine>, cache: &mut PoolCache) {
    for adapter in adapters {
        info!("Discovery: fetching {} pools...", adapter.id());
        match adapter.get_trading_pairs().await {
            Ok(pairs) => {
                info!("Discovery: {} returned {} pairs", adapter.id(), pairs.len());
                cache.update_source(adapter.id(), pairs.clone());
                let trading_pairs = pairs_to_trading(&pairs, adapter.id());
                engine.update_pairs_from_cache(adapter.id(), &trading_pairs).await;
            }
            Err(e) => {
                warn!("Discovery: {} fetch failed: {}", adapter.id(), e);
            }
        }
    }

    if let Err(e) = cache.save(&default_cache_path()) {
        warn!("Failed to save pool cache: {}", e);
    } else {
        info!("Pool cache saved to disk");
    }
}

/// Fast reserve refresh for adapters that support it.
async fn run_reserve_refresh(adapters: &[Arc<dyn DexAdapter>], engine: &Arc<QuoteEngine>) {
    for adapter in adapters {
        match adapter.refresh_reserves().await {
            Ok(n) if n > 0 => {
                info!("Refreshed {} {} pools", n, adapter.id());
                let pairs = adapter.get_cached_pairs().await;
                if !pairs.is_empty() {
                    let trading_pairs = pairs_to_trading(&pairs, adapter.id());
                    engine.update_pairs_from_cache(adapter.id(), &trading_pairs).await;
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!("{} refresh failed: {}", adapter.id(), e);
            }
        }
    }
}

/// Telegram alert only when Soroswap Redis gaps are material (avoids noise on
/// every quote).
fn should_alert_quote_redis_miss(miss: usize, soroswap_refs: usize) -> bool {
    if miss == 0 {
        return false;
    }
    let min_miss = std::env::var("QUOTE_REDIS_MISS_ALERT_MIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    if miss < min_miss {
        return false;
    }
    if soroswap_refs == 0 {
        return true;
    }
    let ratio_bps = miss.saturating_mul(10_000) / soroswap_refs;
    let min_ratio_bps = std::env::var("QUOTE_REDIS_MISS_ALERT_RATIO_BPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000); // 30% of soroswap hops on candidate paths
    ratio_bps >= min_ratio_bps
}

impl AppState {
    fn spawn_price_sampler_if_configured(&self) {
        if let Some(store) = &self.price_store {
            if std::env::var("PRICE_SAMPLER").as_deref() != Ok("0") {
                spawn_price_sampler(self.clone(), store.clone());
            }
        }
    }

    pub async fn current_engine(&self) -> Arc<QuoteEngine> {
        self.engine.read().await.clone()
    }

    /// Find paths, hydrate pool state from Redis, then quote (no path prune; no
    /// RPC by default).
    pub async fn quote_route(&self, request: &RouteRequest) -> OptimalRoute {
        self.quote_route_with_metadata(request).await.route
    }

    /// Quote and retain the age of the oldest pool state used by the quote.
    pub async fn quote_route_with_metadata(&self, request: &RouteRequest) -> QuoteRouteResult {
        let started = std::time::Instant::now();
        let engine = self.current_engine().await;
        let paths = engine.find_candidate_paths(request).await;
        let paths_ms = started.elapsed().as_millis();

        let hydrate_config = PoolHydrateConfig {
            rpc_hydrate_enabled: self.config.quote_rpc_hydrate_enabled,
            max_rpc_pools: self.config.quote_hydrate_max_pools,
        };

        let hydrate_started = std::time::Instant::now();
        let (hydration, redis_miss_xyk, soroswap_refs, oldest_age_ms) = if let Some(store) = &self.pool_state_store {
            pool_hydrate::hydrate_paths(&engine, &paths, store.as_ref(), &self.rpc, &hydrate_config).await
        } else {
            tracing::warn!("pool_state_store missing — Soroban quotes will not hydrate from Redis");
            (router_engine::QuoteHydration::default(), 0, 0, None)
        };
        if should_alert_quote_redis_miss(redis_miss_xyk, soroswap_refs) {
            if let Some(alerter) = &self.telegram {
                let pct = if soroswap_refs > 0 {
                    redis_miss_xyk * 100 / soroswap_refs
                } else {
                    0
                };
                let detail = format!(
                    "quote Redis soroswap misses={redis_miss_xyk}/{soroswap_refs} ({pct}%) paths={} rpc_hydrate={}",
                    paths.len(),
                    hydrate_config.rpc_hydrate_enabled
                );
                let _ = alerter
                    .alert("quote_redis_miss", &format!("⚠️ LumAgg API\n{detail}"))
                    .await;
            }
        }
        let soroban_path_count = paths
            .iter()
            .filter(|p| !p.sources.is_empty() && p.sources.iter().all(|s| s.as_str() != "classic_dex"))
            .count();
        let hydrate_ms = hydrate_started.elapsed().as_millis();
        debug!(
            paths = paths.len(),
            soroban_paths = soroban_path_count,
            xyk_hydrated = hydration.xyk_pools.len(),
            clmm_hydrated = hydration.clmm_pools.len(),
            aquarius_hydrated = hydration.aquarius_pools.len(),
            paths_ms,
            hydrate_ms,
            redis_miss_xyk,
            soroswap_refs,
            oldest_pool_age_ms = oldest_age_ms,
            rpc_hydrate_enabled = hydrate_config.rpc_hydrate_enabled,
            "quote_route hydration"
        );

        // Do not gate public splits on pool age — /quote serves swap UI as well as
        // arb. Freshness is handled by worker refresh→Redis + thin-split filter.
        let quote_started = std::time::Instant::now();
        let route = engine.get_route_with_paths(request, &paths, Some(&hydration)).await;
        debug!(
            quote_ms = quote_started.elapsed().as_millis(),
            total_ms = started.elapsed().as_millis(),
            engine_compute_ms = route.compute_time_ms,
            is_split = route.is_split,
            "quote_route complete"
        );
        QuoteRouteResult {
            route,
            oldest_pool_age_ms: oldest_age_ms,
        }
    }

    fn spawn_snapshot_reloader(
        &self,
        snapshot_store: Arc<dyn SnapshotStore>,
        snapshot_events: Option<mpsc::UnboundedReceiver<SnapshotListenerEvent>>,
        initial_version: Option<String>,
    ) {
        let engine_holder = self.engine.clone();
        let token_metadata = self.token_metadata.clone();
        let snapshot_meta_holder = self.snapshot_meta.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut current_version = initial_version;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                normalize_snapshot_poll_interval_ms(config.snapshot_poll_interval_ms),
            ));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut snapshot_events = snapshot_events;
            let mut reload_mode = if snapshot_events.is_some() {
                SnapshotReloadMode::PollingFallback
            } else {
                SnapshotReloadMode::PollingOnly
            };

            loop {
                let trigger = if let Some(receiver) = snapshot_events.as_mut() {
                    if reload_mode_uses_polling(reload_mode) {
                        tokio::select! {
                            _ = interval.tick() => SnapshotReloadTrigger::PollTick,
                            event = receiver.recv() => match event {
                                Some(event) => SnapshotReloadTrigger::ListenerEvent(event),
                                None => SnapshotReloadTrigger::ListenerClosed,
                            },
                        }
                    } else {
                        match receiver.recv().await {
                            Some(event) => SnapshotReloadTrigger::ListenerEvent(event),
                            None => SnapshotReloadTrigger::ListenerClosed,
                        }
                    }
                } else {
                    interval.tick().await;
                    SnapshotReloadTrigger::PollTick
                };

                match trigger {
                    SnapshotReloadTrigger::PollTick => {}
                    SnapshotReloadTrigger::ListenerClosed => {
                        snapshot_events = None;
                        reload_mode = SnapshotReloadMode::PollingOnly;
                        warn!("Snapshot pub/sub listener stopped, continuing with polling fallback");
                        continue;
                    }
                    SnapshotReloadTrigger::ListenerEvent(event) => {
                        reload_mode = next_snapshot_reload_mode(reload_mode, &event);
                        match event {
                            SnapshotListenerEvent::ListenerHealthy => continue,
                            SnapshotListenerEvent::ListenerDegraded => continue,
                            SnapshotListenerEvent::SnapshotVersion(version) => {
                                if !should_reload_snapshot_version(current_version.as_deref(), &version) {
                                    continue;
                                }
                            }
                        }
                    }
                }

                let snapshot = match snapshot_store.load_current_snapshot().await {
                    Ok(snapshot) => snapshot,
                    Err(e) => {
                        warn!("Failed to reload snapshot from store: {}", e);
                        continue;
                    }
                };

                if !should_reload_snapshot_version(current_version.as_deref(), &snapshot.version) {
                    continue;
                }

                match build_engine_from_snapshot(&config, &snapshot).await {
                    Ok(engine) => {
                        let engine = Arc::new(engine);
                        if let Err(error) = attach_snapshot_live_classic_adapter(&engine).await {
                            warn!("Failed to attach snapshot live adapter: {}", error);
                            continue;
                        }
                        token_metadata.replace_all(snapshot_token_metadata(&snapshot)).await;
                        *engine_holder.write().await = engine;
                        *snapshot_meta_holder.write().await = Some(snapshot.current_meta());
                        current_version = Some(snapshot.version.clone());
                        info!("Reloaded market snapshot version {}", snapshot.version);
                    }
                    Err(e) => {
                        warn!("Failed to build engine from snapshot {}: {}", snapshot.version, e);
                    }
                }
            }
        });
    }

    pub async fn new(config: AppConfig) -> Result<Self> {
        if config.lumagg_mode == LumaggMode::Embedded {
            return new_embedded(config).await;
        }

        let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
        let token_metadata = Arc::new(TokenMetadataStore::new(rpc.clone()));

        if let Some(snapshot_store) = configured_snapshot_store(&config)? {
            let snapshot_events = configured_snapshot_event_listener(&config)?;
            let (engine, initial_version, initial_token_metadata, initial_snapshot_meta) =
                load_initial_snapshot_engine(&config, snapshot_store.as_ref()).await?;
            if let Some(token_metadata_map) = initial_token_metadata {
                token_metadata.replace_all(token_metadata_map).await;
            }
            let pool_state_store = configured_pool_state_store(&config)?;
            let telegram = lumagg_alerts::TelegramAlerter::from_env_api_primary().map(Arc::new);
            let price_store = configured_price_store()?;
            if telegram.is_some() {
                info!("Telegram alerts enabled on API (quote Redis miss)");
            }
            let state = Self {
                engine: Arc::new(RwLock::new(engine)),
                config,
                token_metadata,
                rpc,
                pool_state_store,
                telegram,
                price_store,
                snapshot_meta: Arc::new(RwLock::new(initial_snapshot_meta)),
            };
            state.spawn_snapshot_reloader(snapshot_store, snapshot_events, initial_version);
            state.spawn_price_sampler_if_configured();
            return Ok(state);
        }

        let engine = build_empty_quote_engine(&config);

        let cache_path = default_cache_path();
        match PoolCache::load(&cache_path) {
            Ok(cache) => {
                info!("Loaded pool cache from disk");
                for source in &cache.sources {
                    let trading_pairs: Vec<router_engine::TradingPair> = source
                        .pairs
                        .iter()
                        .map(|p| router_engine::TradingPair {
                            token_a: p.token_a.clone(),
                            token_b: p.token_b.clone(),
                            source: source.source.clone(),
                            pool_address: p.pool_address.clone(),
                            fee_bps: p.fee_bps,
                            reserve_a: p.reserve_a,
                            reserve_b: p.reserve_b,
                        })
                        .collect();
                    let trading_pairs = sanitize_cached_pairs(&source.source, trading_pairs);

                    engine.update_pairs_from_cache(&source.source, &trading_pairs).await;
                    info!("Loaded {} cached pairs for {}", trading_pairs.len(), source.source);
                }
            }
            Err(_) => {
                info!("No pool cache found, will fetch from chain");
            }
        }

        let engine_bg = engine.clone();
        let token_metadata_bg = token_metadata.clone();
        let refresh_interval = config.refresh_interval_secs;
        let discovery_interval = config.discovery_interval_secs;

        let rpc_bg = rpc.clone();
        tokio::spawn(async move {
            let adapters: Vec<Arc<dyn DexAdapter>> = vec![
                Arc::new(SoroswapAdapter::new(rpc_bg.clone())),
                Arc::new(AquariusAdapter::new(rpc_bg.clone())),
                Arc::new(PhoenixAdapter::new(rpc_bg.clone())),
                Arc::new(SushiAdapter::new(rpc_bg.clone())),
                Arc::new(CometAdapter::new(rpc_bg.clone())),
                Arc::new(ClassicDexAdapter::new(None)),
                Arc::new(AquariusClmmAdapter::new(rpc_bg.clone())),
            ];

            for adapter in &adapters {
                engine_bg.register_adapter(adapter.clone()).await;
            }

            let mut cache = PoolCache::default();
            run_discovery(&adapters, &engine_bg, &mut cache).await;

            let all_tokens = engine_bg.get_all_tokens().await;
            token_metadata_bg.resolve_unknown(all_tokens).await;

            info!(
                "Background: discovery every {}s, reserve refresh every {}s",
                discovery_interval, refresh_interval
            );

            let engine_refresh = engine_bg.clone();
            let adapters_refresh = adapters.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(refresh_interval));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    run_reserve_refresh(&adapters_refresh, &engine_refresh).await;
                }
            });

            let mut discovery_timer = tokio::time::interval(std::time::Duration::from_secs(discovery_interval));
            discovery_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Initial discovery already ran; first periodic tick fires after one interval.
            discovery_timer.tick().await;

            loop {
                discovery_timer.tick().await;
                let mut cache = PoolCache::default();
                run_discovery(&adapters, &engine_bg, &mut cache).await;
                let all_tokens = engine_bg.get_all_tokens().await;
                token_metadata_bg.resolve_unknown(all_tokens).await;
            }
        });

        let pool_state_store = configured_pool_state_store(&config)?;
        let telegram = lumagg_alerts::TelegramAlerter::from_env_api_primary().map(Arc::new);
        let price_store = configured_price_store()?;
        let state = Self {
            engine: Arc::new(RwLock::new(engine)),
            config,
            token_metadata,
            rpc,
            pool_state_store,
            telegram,
            price_store,
            snapshot_meta: Arc::new(RwLock::new(None)),
        };
        state.spawn_price_sampler_if_configured();
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        anyhow::anyhow,
        async_trait::async_trait,
        dex_adapters::{AdapterQuote, AdapterTradingPair, ProtocolType, SwapOperation},
        market_snapshot::{store::SnapshotListenerEvent, MarketSnapshot},
        router_engine::TokenId,
    };

    struct FailingSnapshotStore;

    struct StaticQuoteAdapter;

    #[async_trait]
    impl SnapshotStore for FailingSnapshotStore {
        async fn load_current_snapshot(&self) -> Result<MarketSnapshot> {
            Err(anyhow!("snapshot missing"))
        }

        async fn publish_snapshot(&self, _snapshot: &MarketSnapshot) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl DexAdapter for StaticQuoteAdapter {
        fn id(&self) -> &str {
            "classic_dex"
        }

        fn name(&self) -> &str {
            "Static Classic"
        }

        fn protocol_type(&self) -> ProtocolType {
            ProtocolType::ClassicDex
        }

        async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
            Ok(vec![AdapterTradingPair {
                token_a: TokenId::from_str_auto("token-a"),
                token_b: TokenId::from_str_auto("token-b"),
                pool_address: "classic-pool".to_string(),
                fee_bps: 0,
                reserve_a: None,
                reserve_b: None,
            }])
        }

        async fn get_quote(
            &self,
            _token_in: &TokenId,
            _token_out: &TokenId,
            amount_in: u128,
            _pool_address: &str,
        ) -> Result<Option<AdapterQuote>> {
            Ok(Some(AdapterQuote {
                amount_out: amount_in + 123,
                fee_bps: 0,
                price_impact_bps: 0,
            }))
        }

        async fn build_swap_op(
            &self,
            _token_in: &TokenId,
            _token_out: &TokenId,
            _amount_in: u128,
            _min_amount_out: u128,
            _pool_address: &str,
        ) -> Result<SwapOperation> {
            Ok(SwapOperation::ClassicPathPayment {
                send_asset: "native".to_string(),
                dest_asset: "USDC".to_string(),
                send_amount: 1,
                dest_min: 1,
                path: vec![],
            })
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    #[test]
    fn healthy_pubsub_mode_does_not_poll() {
        assert!(!reload_mode_uses_polling(SnapshotReloadMode::PubSubHealthy));
        assert!(reload_mode_uses_polling(SnapshotReloadMode::PollingFallback));
        assert!(reload_mode_uses_polling(SnapshotReloadMode::PollingOnly));
    }

    #[test]
    fn zero_poll_interval_is_normalized() {
        assert_eq!(normalize_snapshot_poll_interval_ms(0), 1);
        assert_eq!(normalize_snapshot_poll_interval_ms(25), 25);
    }

    #[test]
    fn listener_degrade_and_recovery_switch_reload_modes() {
        let degraded = next_snapshot_reload_mode(
            SnapshotReloadMode::PubSubHealthy,
            &SnapshotListenerEvent::ListenerDegraded,
        );
        assert_eq!(degraded, SnapshotReloadMode::PollingFallback);

        let recovered = next_snapshot_reload_mode(
            SnapshotReloadMode::PollingFallback,
            &SnapshotListenerEvent::ListenerHealthy,
        );
        assert_eq!(recovered, SnapshotReloadMode::PubSubHealthy);
    }

    #[tokio::test]
    async fn initial_snapshot_load_failure_uses_empty_engine() {
        let config = AppConfig::default();
        let (engine, current_version, token_metadata, snapshot_meta) =
            load_initial_snapshot_engine(&config, &FailingSnapshotStore)
                .await
                .unwrap();

        let route = engine
            .get_route(&router_engine::RouteRequest {
                token_in: TokenId::from_str_auto("token-a"),
                token_out: TokenId::from_str_auto("token-b"),
                amount_in: 1_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_soroban: None,
            })
            .await;

        assert!(route.sub_orders.is_empty());
        assert!(current_version.is_none());
        assert!(token_metadata.is_none());
        assert!(snapshot_meta.is_none());
        assert_eq!(current_version, None);
        assert!(token_metadata.is_none());
    }

    #[tokio::test]
    async fn attaches_live_snapshot_adapter_to_engine() {
        let engine = build_empty_quote_engine(&AppConfig::default());
        attach_snapshot_live_adapter(&engine, Arc::new(StaticQuoteAdapter))
            .await
            .unwrap();

        let route = engine
            .get_route(&router_engine::RouteRequest {
                token_in: TokenId::from_str_auto("token-a"),
                token_out: TokenId::from_str_auto("token-b"),
                amount_in: 1_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_soroban: None,
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert_eq!(route.total_expected_out, 1_123);
    }
}

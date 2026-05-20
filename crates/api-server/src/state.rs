use anyhow::Result;
use dex_adapters::{
    aquarius::AquariusAdapter,
    aquarius_clmm::AquariusClmmAdapter,
    cache::{default_cache_path, PoolCache},
    classic_dex::ClassicDexAdapter,
    comet::CometAdapter,
    phoenix::PhoenixAdapter,
    rpc::SorobanRpc,
    soroswap::SoroswapAdapter,
    sushi::SushiAdapter,
    token_metadata::TokenMetadataStore,
    traits::AdapterTradingPair,
    DexAdapter,
};
use router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine};
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::AppConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<QuoteEngine>,
    pub config: AppConfig,
    pub token_metadata: Arc<TokenMetadataStore>,
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

/// Full pool discovery for all adapters: replace graph edges per source and persist cache.
async fn run_discovery(
    adapters: &[Arc<dyn DexAdapter>],
    engine: &Arc<QuoteEngine>,
    cache: &mut PoolCache,
) {
    for adapter in adapters {
        info!("Discovery: fetching {} pools...", adapter.id());
        match adapter.get_trading_pairs().await {
            Ok(pairs) => {
                info!(
                    "Discovery: {} returned {} pairs",
                    adapter.id(),
                    pairs.len()
                );
                cache.update_source(adapter.id(), pairs.clone());
                let trading_pairs = pairs_to_trading(&pairs, adapter.id());
                engine
                    .update_pairs_from_cache(adapter.id(), &trading_pairs)
                    .await;
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
                    engine
                        .update_pairs_from_cache(adapter.id(), &trading_pairs)
                        .await;
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!("{} refresh failed: {}", adapter.id(), e);
            }
        }
    }
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let path_finder_config = PathFinderConfig::default();
        let split_config = SplitConfig::default();

        let engine = Arc::new(QuoteEngine::new(path_finder_config, split_config));

        let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
        let token_metadata = Arc::new(TokenMetadataStore::new(rpc.clone()));

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

                    engine
                        .update_pairs_from_cache(&source.source, &trading_pairs)
                        .await;
                    info!(
                        "Loaded {} cached pairs for {}",
                        trading_pairs.len(),
                        source.source
                    );
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

        tokio::spawn(async move {
            let adapters: Vec<Arc<dyn DexAdapter>> = vec![
                Arc::new(SoroswapAdapter::new(rpc.clone())),
                Arc::new(AquariusAdapter::new(rpc.clone())),
                Arc::new(PhoenixAdapter::new(rpc.clone())),
                Arc::new(SushiAdapter::new(rpc.clone())),
                Arc::new(CometAdapter::new(rpc.clone())),
                Arc::new(ClassicDexAdapter::new(None)),
                Arc::new(AquariusClmmAdapter::new(rpc.clone())),
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
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_secs(refresh_interval));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    run_reserve_refresh(&adapters_refresh, &engine_refresh).await;
                }
            });

            let mut discovery_timer =
                tokio::time::interval(std::time::Duration::from_secs(discovery_interval));
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

        Ok(Self {
            engine,
            config,
            token_metadata,
        })
    }
}

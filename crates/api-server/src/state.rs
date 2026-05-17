use anyhow::Result;
use dex_adapters::{
    aquarius::AquariusAdapter,
    cache::{PoolCache, default_cache_path},
    classic_dex::ClassicDexAdapter,
    phoenix::PhoenixAdapter,
    rpc::SorobanRpc,
    soroswap::SoroswapAdapter,
    sushi::SushiAdapter,
    AdapterTradingPair, DexAdapter,
};
use router_engine::{
    path_finder::PathFinderConfig,
    split_optimizer::SplitConfig,
    QuoteEngine,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::config::AppConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<QuoteEngine>,
    pub config: AppConfig,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let path_finder_config = PathFinderConfig::default();
        let split_config = SplitConfig::default();

        let engine = Arc::new(QuoteEngine::new(path_finder_config, split_config));

        // Create shared RPC client
        let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));

        // Try to load cached pool data for instant startup
        let cache_path = default_cache_path();
        let has_cache = match PoolCache::load(&cache_path) {
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

                    engine.update_pairs_from_cache(&source.source, &trading_pairs).await;
                    info!("Loaded {} cached pairs for {}", trading_pairs.len(), source.source);
                }
                true
            }
            Err(_) => {
                info!("No pool cache found, will fetch from chain");
                false
            }
        };

        // Spawn background task to fetch fresh data from chain
        let engine_clone = engine.clone();
        let rpc_clone = rpc.clone();
        let refresh_interval = config.refresh_interval_secs;

        tokio::spawn(async move {
            let adapters: Vec<Arc<dyn DexAdapter>> = vec![
                Arc::new(SoroswapAdapter::new(rpc_clone.clone())),
                Arc::new(AquariusAdapter::new(rpc_clone.clone())),
                Arc::new(PhoenixAdapter::new(rpc_clone.clone())),
                Arc::new(SushiAdapter::new(rpc_clone.clone())),
                Arc::new(ClassicDexAdapter::new(None)), // Uses public Horizon API
            ];

            // Initial registration (fetches from chain)
            let mut cache = PoolCache::default();

            for adapter in &adapters {
                info!("Background: fetching {} pools...", adapter.id());
                match adapter.get_trading_pairs().await {
                    Ok(pairs) => {
                        info!("Background: {} returned {} pairs", adapter.id(), pairs.len());
                        cache.update_source(adapter.id(), pairs);
                    }
                    Err(e) => {
                        warn!("Background: {} fetch failed: {}", adapter.id(), e);
                    }
                }

                // Register adapter (for future quote calls)
                engine_clone.register_adapter(adapter.clone()).await;
            }

            // Save cache to disk
            if let Err(e) = cache.save(&default_cache_path()) {
                warn!("Failed to save pool cache: {}", e);
            } else {
                info!("Pool cache saved to disk");
            }

            info!("Background: initial load complete. Starting refresh loop ({}s interval).", refresh_interval);

            // Periodic refresh — use batch getLedgerEntries for fast reserves update
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(refresh_interval)).await;

                // Fast batch refresh for each adapter
                for adapter in &adapters {
                    match adapter.refresh_reserves().await {
                        Ok(n) if n > 0 => {
                            info!("Refreshed {} {} pools", n, adapter.id());
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!("{} refresh failed: {}", adapter.id(), e);
                        }
                    }
                }

                // Update path finder graph with new reserves
                engine_clone.refresh_pairs().await;
            }
        });

        Ok(Self { engine, config })
    }
}

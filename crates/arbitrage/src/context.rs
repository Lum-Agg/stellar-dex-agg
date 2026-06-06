//! Load Redis snapshot and build a quote engine for Soroban paths only.

use {
    crate::config::ArbConfig,
    anyhow::{Context, Result},
    market_snapshot::{
        pool_state_store::build_pool_state_store,
        store::{build_snapshot_store, SnapshotStoreBackend},
        MarketSnapshot, TradingPairSnapshot,
    },
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine, TradingPair},
    std::sync::Arc,
};

const SKIP_SOURCES: &[&str] = &["classic_dex", "comet"];

fn snapshot_pair_to_trading(pair: &TradingPairSnapshot, source: &str) -> TradingPair {
    TradingPair {
        token_a: router_engine::TokenId::from_str_auto(&pair.token_a),
        token_b: router_engine::TokenId::from_str_auto(&pair.token_b),
        source: source.to_string(),
        pool_address: pair.pool_address.clone(),
        fee_bps: pair.fee_bps,
        reserve_a: None,
        reserve_b: None,
    }
}

pub struct ArbContext {
    pub config: ArbConfig,
    pub snapshot: MarketSnapshot,
    pub engine: QuoteEngine,
    pub pool_store: Arc<market_snapshot::pool_state_store::RedisPoolStateStore>,
}

impl ArbContext {
    pub async fn connect(config: ArbConfig) -> Result<Self> {
        let snapshot_store = build_snapshot_store(
            SnapshotStoreBackend::Redis,
            None,
            Some(config.snapshot_redis_url.as_str()),
            None,
            None,
        )?;
        let snapshot = snapshot_store
            .load_current_snapshot()
            .await
            .context("load lumagg:snapshot:current from Redis")?;

        let pool_store = Arc::new(build_pool_state_store(config.pool_state_redis_url.as_str())?);

        let engine = build_soroban_engine(&snapshot).await?;

        Ok(Self {
            config,
            snapshot,
            engine,
            pool_store,
        })
    }
}

async fn build_soroban_engine(snapshot: &MarketSnapshot) -> Result<QuoteEngine> {
    let pf_config = PathFinderConfig {
        max_hops: 4,
        max_multi_hop_paths: 80,
        max_direct_paths: 0,
        bridge_tokens: PathFinderConfig::default().bridge_tokens,
    };
    let engine = QuoteEngine::new(pf_config, SplitConfig::default());

    for source in &snapshot.sources {
        if SKIP_SOURCES.contains(&source.source.as_str()) {
            continue;
        }
        let pairs: Vec<TradingPair> = source
            .pairs
            .iter()
            .map(|p| snapshot_pair_to_trading(p, &source.source))
            .collect();
        engine.update_pairs_from_cache(&source.source, &pairs).await;
    }

    Ok(engine)
}

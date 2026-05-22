//! Batched pool-state hydration for `/quote` (Redis MGET + xy=k RPC fallback).

use std::collections::{HashMap, HashSet};

use dex_adapters::{
    batch_refresh::batch_refresh_soroswap_reserves,
    clmm_math::clmm_pool_from_snapshot,
    comet::CometAdapter,
    rpc::SorobanRpc,
};
use std::sync::Arc;
use market_snapshot::{
    pool_state_store::{parse_quote_hydrate_max_pools_from_env, RedisPoolStateStore, XykPoolStateValue},
    ClmmPoolSnapshot,
};
use router_engine::{Path, QuoteEngine, QuoteHydration, SnapshotClmmQuoteState};
use tracing::debug;

const CLMM_SOURCES: &[&str] = &["sushi", "aquarius_clmm"];
const BATCH_XYK_SOURCES: &[&str] = &["soroswap", "aquarius"];

pub struct PoolHydrateConfig {
    pub max_rpc_pools: usize,
}

impl Default for PoolHydrateConfig {
    fn default() -> Self {
        Self {
            max_rpc_pools: parse_quote_hydrate_max_pools_from_env(),
        }
    }
}

fn collect_pool_refs(paths: &[Path]) -> (Vec<(String, String)>, Vec<(String, String)>, Vec<String>) {
    let mut xyk = HashSet::new();
    let mut clmm = HashSet::new();
    let mut comet = HashSet::new();

    for path in paths {
        for (source, pool_address) in path.sources.iter().zip(path.pool_addresses.iter()) {
            if source == "classic_dex" {
                continue;
            }
            if source == "comet" {
                comet.insert(pool_address.clone());
            } else if CLMM_SOURCES.contains(&source.as_str()) {
                clmm.insert((source.clone(), pool_address.clone()));
            } else {
                xyk.insert((source.clone(), pool_address.clone()));
            }
        }
    }

    let mut xyk: Vec<_> = xyk.into_iter().collect();
    let mut clmm: Vec<_> = clmm.into_iter().collect();
    let mut comet: Vec<_> = comet.into_iter().collect();
    xyk.sort();
    clmm.sort();
    comet.sort();
    (xyk, clmm, comet)
}

fn clmm_state_from_snapshot(snapshot: &ClmmPoolSnapshot) -> SnapshotClmmQuoteState {
    let (pool, ticks) = clmm_pool_from_snapshot(snapshot);
    SnapshotClmmQuoteState {
        source: snapshot.source.clone(),
        pool_address: snapshot.pool_address.clone(),
        is_complete: snapshot
            .coverage
            .as_ref()
            .map(|c| c.is_complete)
            .unwrap_or(false),
        pool,
        ticks,
        coverage: snapshot.coverage.clone(),
    }
}

/// Load per-pool state for candidate paths: Redis first, then batched xy=k RPC for misses.
pub async fn hydrate_paths(
    engine: &QuoteEngine,
    paths: &[Path],
    store: &RedisPoolStateStore,
    rpc: &SorobanRpc,
    config: &PoolHydrateConfig,
) -> QuoteHydration {
    let (xyk_refs, clmm_refs, comet_pools) = collect_pool_refs(paths);
    if xyk_refs.is_empty() && clmm_refs.is_empty() && comet_pools.is_empty() {
        return QuoteHydration::default();
    }

    let mut xyk_pools = store.fetch_xyk(&xyk_refs).await.unwrap_or_default();
    let clmm_snapshots = store.fetch_clmm(&clmm_refs).await.unwrap_or_default();

    let clmm_pools: HashMap<String, SnapshotClmmQuoteState> = clmm_snapshots
        .into_iter()
        .map(|(key, snapshot)| (key, clmm_state_from_snapshot(&snapshot)))
        .collect();

    let mut rpc_candidates: Vec<(String, String)> = Vec::new();
    for (source, pool_address) in &xyk_refs {
        let key = XykPoolStateValue::pool_key(source, pool_address);
        if !xyk_pools.contains_key(&key) && BATCH_XYK_SOURCES.contains(&source.as_str()) {
            rpc_candidates.push((source.clone(), pool_address.clone()));
        }
    }
    rpc_candidates.truncate(config.max_rpc_pools);

    if !rpc_candidates.is_empty() {
        let pool_addresses: Vec<String> = rpc_candidates
            .iter()
            .map(|(_, pool)| pool.clone())
            .collect();
        match batch_refresh_soroswap_reserves(rpc, &pool_addresses).await {
            Ok(results) => {
                let cached = engine.cached_pool_edges().await;
                let mut writeback: Vec<XykPoolStateValue> = Vec::new();

                for ((source, pool_address), (_, reserves)) in
                    rpc_candidates.iter().zip(results.iter())
                {
                    let Some((r0, r1)) = *reserves else {
                        continue;
                    };
                    let Some(edge) = cached.iter().find(|p| {
                        p.source == *source && p.pool_address == *pool_address
                    }) else {
                        continue;
                    };
                    let value = xyk_value_from_batch(edge, r0, r1, source, pool_address);
                    let key = XykPoolStateValue::pool_key(source, pool_address);
                    xyk_pools.insert(key, value.clone());
                    writeback.push(value);
                }

                if !writeback.is_empty() {
                    if let Err(error) = store.set_xyk_batch(&writeback).await {
                        debug!("xy=k hydrate writeback failed: {}", error);
                    }
                }
            }
            Err(error) => debug!("xy=k batch hydrate RPC failed: {}", error),
        }
    }

    let mut comet_states = HashMap::new();
    if !comet_pools.is_empty() {
        let comet = CometAdapter::new(Arc::new(SorobanRpc::new(
            rpc.url(),
            rpc.network_passphrase(),
        )));
        for pool_address in comet_pools {
            match comet.fetch_pool_quote_state(&pool_address).await {
                Ok(state) => {
                    comet_states.insert(pool_address, state);
                }
                Err(error) => {
                    debug!("Comet hydrate failed for {}: {}", pool_address, error);
                }
            }
        }
    }

    debug!(
        xyk = xyk_pools.len(),
        clmm = clmm_pools.len(),
        comet = comet_states.len(),
        "Quote pool hydration ready"
    );

    QuoteHydration {
        xyk_pools,
        clmm_pools,
        comet_pools: comet_states,
    }
}

fn xyk_value_from_batch(
    edge: &router_engine::TradingPair,
    reserve0: u128,
    reserve1: u128,
    source: &str,
    pool_address: &str,
) -> XykPoolStateValue {
    // Matches soroswap/aquarius batch refresh: contract reserve0/1 → pair reserve_a/b.
    XykPoolStateValue {
        source: source.to_string(),
        pool_address: pool_address.to_string(),
        token_a: edge.token_a.canonical(),
        token_b: edge.token_b.canonical(),
        fee_bps: edge.fee_bps,
        reserve_a: reserve0,
        reserve_b: reserve1,
    }
}

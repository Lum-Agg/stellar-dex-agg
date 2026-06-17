//! Batched pool-state hydration for `/quote` (Redis MGET; optional RPC
//! fallback).

use {
    dex_adapters::{
        batch_refresh::batch_refresh_soroswap_reserves, clmm_math::clmm_pool_from_snapshot, comet::CometAdapter,
        comet_math::CometRecord, rpc::SorobanRpc, AquariusPoolQuoteState, CometPoolQuoteState,
    },
    market_snapshot::{
        pool_state_store::{
            parse_quote_hydrate_max_pools_from_env, AquariusPoolStateValue, CometPoolStateValue,
            RedisPoolStateStore, XykPoolStateValue,
        },
        ClmmPoolSnapshot,
    },
    router_engine::{Path, QuoteEngine, QuoteHydration, SnapshotClmmQuoteState},
    std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    },
    tracing::{debug, warn},
};

const CLMM_SOURCES: &[&str] = &["sushi", "aquarius_clmm"];
const BATCH_XYK_SOURCES: &[&str] = &["soroswap"];

pub struct PoolHydrateConfig {
    /// When false, `/quote` uses Redis only (worker is the sole writer).
    pub rpc_hydrate_enabled: bool,
    pub max_rpc_pools: usize,
}

impl Default for PoolHydrateConfig {
    fn default() -> Self {
        Self {
            rpc_hydrate_enabled: false,
            max_rpc_pools: parse_quote_hydrate_max_pools_from_env(),
        }
    }
}

fn collect_pool_refs(paths: &[Path]) -> (Vec<(String, String)>, Vec<(String, String)>, Vec<String>, Vec<String>) {
    let mut xyk = HashSet::new();
    let mut clmm = HashSet::new();
    let mut comet = HashSet::new();
    let mut aquarius = HashSet::new();

    for path in paths {
        for (source, pool_address) in path.sources.iter().zip(path.pool_addresses.iter()) {
            if source == "classic_dex" {
                continue;
            }
            if source == "comet" {
                comet.insert(pool_address.clone());
            } else if source == "aquarius" {
                aquarius.insert(pool_address.clone());
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
    let mut aquarius: Vec<_> = aquarius.into_iter().collect();
    xyk.sort();
    clmm.sort();
    comet.sort();
    aquarius.sort();
    (xyk, clmm, comet, aquarius)
}

fn clmm_state_from_snapshot(snapshot: &ClmmPoolSnapshot) -> SnapshotClmmQuoteState {
    let (pool, ticks) = clmm_pool_from_snapshot(snapshot);
    SnapshotClmmQuoteState {
        source: snapshot.source.clone(),
        pool_address: snapshot.pool_address.clone(),
        is_complete: snapshot.coverage.as_ref().map(|c| c.is_complete).unwrap_or(false),
        pool,
        ticks,
        coverage: snapshot.coverage.clone(),
    }
}

fn aquarius_quote_state(value: &AquariusPoolStateValue) -> AquariusPoolQuoteState {
    AquariusPoolQuoteState {
        pool_address: value.pool_address.clone(),
        tokens: value.tokens.clone(),
        reserves: value.reserves.clone(),
        fee_bps: value.fee_bps,
        is_stable: value.is_stable,
        amp: value.amp,
    }
}

fn comet_quote_state(value: &CometPoolStateValue) -> CometPoolQuoteState {
    CometPoolQuoteState {
        records: value
            .records
            .iter()
            .map(|(token, record)| {
                (
                    token.clone(),
                    CometRecord {
                        balance: record.balance,
                        weight: record.weight,
                        scalar: record.scalar,
                    },
                )
            })
            .collect(),
        swap_fee: value.swap_fee,
    }
}

fn comet_state_to_value(pool_address: &str, state: &CometPoolQuoteState) -> CometPoolStateValue {
    use market_snapshot::pool_state_store::CometTokenRecordValue;
    CometPoolStateValue {
        pool_address: pool_address.to_string(),
        records: state
            .records
            .iter()
            .map(|(token, record)| {
                (
                    token.clone(),
                    CometTokenRecordValue {
                        balance: record.balance,
                        weight: record.weight,
                        scalar: record.scalar,
                    },
                )
            })
            .collect(),
        swap_fee: state.swap_fee,
    }
}

/// Load per-pool state for candidate paths from Redis; optional batched xy=k
/// RPC for misses.
pub async fn hydrate_paths(
    engine: &QuoteEngine,
    paths: &[Path],
    store: &RedisPoolStateStore,
    rpc: &SorobanRpc,
    config: &PoolHydrateConfig,
) -> (QuoteHydration, usize, usize) {
    let (xyk_refs, clmm_refs, comet_pools, aquarius_refs) = collect_pool_refs(paths);
    let soroswap_ref_count = xyk_refs.iter().filter(|(s, _)| s == "soroswap").count();
    if xyk_refs.is_empty() && clmm_refs.is_empty() && comet_pools.is_empty() && aquarius_refs.is_empty() {
        return (QuoteHydration::default(), 0, 0);
    }

    let mut xyk_pools = store.fetch_xyk(&xyk_refs).await.unwrap_or_default();
    let clmm_snapshots = store.fetch_clmm(&clmm_refs).await.unwrap_or_default();
    let aquarius_raw = store.fetch_aquarius(&aquarius_refs).await.unwrap_or_default();
    let comet_raw = store.fetch_comet(&comet_pools).await.unwrap_or_default();

    let clmm_pools: HashMap<String, SnapshotClmmQuoteState> = clmm_snapshots
        .into_iter()
        .map(|(key, snapshot)| (key, clmm_state_from_snapshot(&snapshot)))
        .collect();

    let aquarius_pools: HashMap<String, AquariusPoolQuoteState> = aquarius_raw
        .into_iter()
        .map(|(pool, value)| (pool, aquarius_quote_state(&value)))
        .collect();

    let mut redis_miss_xyk = 0usize;
    if config.rpc_hydrate_enabled {
        let mut rpc_candidates: Vec<(String, String)> = Vec::new();
        for (source, pool_address) in &xyk_refs {
            let key = XykPoolStateValue::pool_key(source, pool_address);
            if !xyk_pools.contains_key(&key) && BATCH_XYK_SOURCES.contains(&source.as_str()) {
                redis_miss_xyk += 1;
                rpc_candidates.push((source.clone(), pool_address.clone()));
            }
        }
        rpc_candidates.truncate(config.max_rpc_pools);

        if !rpc_candidates.is_empty() {
            let pool_addresses: Vec<String> = rpc_candidates.iter().map(|(_, pool)| pool.clone()).collect();
            match batch_refresh_soroswap_reserves(rpc, &pool_addresses).await {
                Ok(results) => {
                    let cached = engine.cached_pool_edges().await;
                    let mut writeback: Vec<XykPoolStateValue> = Vec::new();

                    for ((source, pool_address), (_, reserves)) in rpc_candidates.iter().zip(results.iter()) {
                        let Some((r0, r1)) = *reserves else {
                            continue;
                        };
                        let Some(edge) = cached
                            .iter()
                            .find(|p| p.source == *source && p.pool_address == *pool_address)
                        else {
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
    } else {
        for (source, pool_address) in &xyk_refs {
            let key = XykPoolStateValue::pool_key(source, pool_address);
            if !xyk_pools.contains_key(&key) && BATCH_XYK_SOURCES.contains(&source.as_str()) {
                redis_miss_xyk += 1;
            }
        }
    }

    let mut redis_miss_aquarius = 0usize;
    for pool_address in &aquarius_refs {
        if !aquarius_pools.contains_key(pool_address) {
            redis_miss_aquarius += 1;
        }
    }
    if redis_miss_aquarius > 0 {
        warn!(
            redis_miss_aquarius,
            paths = paths.len(),
            "quote hydration: Aquarius Redis misses (worker should publish these pools)"
        );
    }

    let mut comet_states: HashMap<String, CometPoolQuoteState> = comet_raw
        .into_iter()
        .map(|(pool, value)| (pool, comet_quote_state(&value)))
        .collect();

    let mut redis_miss_comet = 0usize;
    for pool_address in &comet_pools {
        if !comet_states.contains_key(pool_address) {
            redis_miss_comet += 1;
        }
    }
    if redis_miss_comet > 0 {
        warn!(
            redis_miss_comet,
            paths = paths.len(),
            "quote hydration: Comet Redis misses (worker should publish weighted pool state)"
        );
    }

    if config.rpc_hydrate_enabled && redis_miss_comet > 0 {
        let comet = CometAdapter::new(Arc::new(SorobanRpc::new(rpc.url(), rpc.network_passphrase())));
        let mut rpc_candidates: Vec<String> = comet_pools
            .iter()
            .filter(|pool| !comet_states.contains_key(*pool))
            .cloned()
            .collect();
        rpc_candidates.truncate(config.max_rpc_pools);
        let mut writeback = Vec::new();
        for pool_address in rpc_candidates {
            match comet.fetch_pool_quote_state(&pool_address).await {
                Ok(state) => {
                    comet_states.insert(pool_address.clone(), state.clone());
                    writeback.push(comet_state_to_value(&pool_address, &state));
                }
                Err(error) => {
                    debug!("Comet hydrate failed for {}: {}", pool_address, error);
                }
            }
        }
        if !writeback.is_empty() {
            if let Err(error) = store.set_comet_batch(&writeback).await {
                debug!("Comet hydrate writeback failed: {}", error);
            }
        }
    }

    if redis_miss_xyk > 0 {
        warn!(
            redis_miss_xyk,
            paths = paths.len(),
            rpc_hydrate_enabled = config.rpc_hydrate_enabled,
            "quote hydration: xy=k Redis misses (worker should publish these pools)"
        );
    }

    debug!(
        xyk = xyk_pools.len(),
        clmm = clmm_pools.len(),
        comet = comet_states.len(),
        aquarius = aquarius_pools.len(),
        redis_miss_xyk,
        redis_miss_aquarius,
        redis_miss_comet,
        "hydrated pools for quote"
    );

    (
        QuoteHydration {
            xyk_pools,
            clmm_pools,
            comet_pools: comet_states,
            aquarius_pools,
        },
        redis_miss_xyk,
        soroswap_ref_count,
    )
}

fn xyk_value_from_batch(
    edge: &router_engine::TradingPair,
    r0: u128,
    r1: u128,
    source: &str,
    pool_address: &str,
) -> XykPoolStateValue {
    XykPoolStateValue::new(
        source,
        pool_address,
        &edge.token_a.canonical(),
        &edge.token_b.canonical(),
        edge.fee_bps,
        r0,
        r1,
    )
}

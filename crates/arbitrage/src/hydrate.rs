//! Redis pool-state hydration for cycle quotes (mirrors api-server
//! pool_hydrate, no RPC).

use {
    dex_adapters::{
        clmm_math::clmm_pool_from_snapshot, comet_math::CometRecord, AquariusPoolQuoteState, CometPoolQuoteState,
    },
    market_snapshot::{
        pool_state_store::{AquariusPoolStateValue, CometPoolStateValue, RedisPoolStateStore, XykPoolStateValue},
        ClmmPoolSnapshot,
    },
    router_engine::{Path, QuoteHydration, SnapshotClmmQuoteState},
    std::collections::{HashMap, HashSet},
};

const CLMM_SOURCES: &[&str] = &["sushi", "aquarius_clmm"];

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

    (
        xyk.into_iter().collect(),
        clmm.into_iter().collect(),
        comet.into_iter().collect(),
        aquarius.into_iter().collect(),
    )
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

pub async fn hydrate_paths(paths: &[Path], store: &RedisPoolStateStore) -> (QuoteHydration, usize) {
    let (xyk_refs, clmm_refs, comet_refs, aquarius_refs) = collect_pool_refs(paths);
    if xyk_refs.is_empty() && clmm_refs.is_empty() && comet_refs.is_empty() && aquarius_refs.is_empty() {
        return (QuoteHydration::default(), 0);
    }

    let xyk_pools = store.fetch_xyk(&xyk_refs).await.unwrap_or_default();
    let clmm_snapshots = store.fetch_clmm(&clmm_refs).await.unwrap_or_default();
    let comet_raw = store.fetch_comet(&comet_refs).await.unwrap_or_default();
    let aquarius_raw = store.fetch_aquarius(&aquarius_refs).await.unwrap_or_default();

    let clmm_pools: HashMap<String, SnapshotClmmQuoteState> = clmm_snapshots
        .into_iter()
        .map(|(key, snapshot)| (key, clmm_state_from_snapshot(&snapshot)))
        .collect();

    let comet_pools: HashMap<String, CometPoolQuoteState> = comet_raw
        .into_iter()
        .map(|(pool, value)| (pool, comet_quote_state(&value)))
        .collect();

    let aquarius_pools: HashMap<String, AquariusPoolQuoteState> = aquarius_raw
        .into_iter()
        .map(|(pool, value)| (pool, aquarius_quote_state(&value)))
        .collect();

    let mut redis_miss = 0usize;
    for (source, pool_address) in &xyk_refs {
        let key = XykPoolStateValue::pool_key(source, pool_address);
        if !xyk_pools.contains_key(&key) {
            redis_miss += 1;
        }
    }
    for pool_address in &comet_refs {
        if !comet_pools.contains_key(pool_address) {
            redis_miss += 1;
        }
    }

    (
        QuoteHydration {
            xyk_pools,
            clmm_pools,
            comet_pools,
            aquarius_pools,
        },
        redis_miss,
    )
}

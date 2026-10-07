//! Refresh only ledger-touched pools and push updates to Redis.

use {
    crate::{
        clmm_metrics::ClmmCoverageMetrics, ledger_watcher::ledger_max_touched_refresh_from_env,
        pool_state_publish::comet_state_to_value,
    },
    anyhow::Result,
    dex_adapters::{
        aquarius::AquariusAdapter, aquarius_clmm::AquariusClmmAdapter, batch_refresh::batch_refresh_soroswap_reserves,
        comet::CometAdapter, phoenix::PhoenixAdapter, pool_index::PoolRef, rpc::SorobanRpc, soroswap::SoroswapAdapter,
        sushi::SushiAdapter, DexAdapter,
    },
    market_snapshot::{
        pool_state_store::{
            should_publish_clmm_to_redis, AquariusPoolStateValue, CometPoolStateValue, PoolStateStore,
            XykPoolStateValue,
        },
        ClmmPoolSnapshot, SourceSnapshot,
    },
    std::collections::{HashMap, HashSet},
    tracing::{debug, warn},
};

const BATCH_XYK_SOURCES: &[&str] = &["soroswap"];
const AQUARIUS_SOURCE: &str = "aquarius";
const CLMM_SOURCES: &[&str] = &["sushi", "aquarius_clmm"];
const PHOENIX_SOURCE: &str = "phoenix";
const COMET_SOURCE: &str = "comet";

pub struct TouchedRefreshContext<'a> {
    pub rpc: &'a SorobanRpc,
    pub pool_store: &'a dyn PoolStateStore,
    pub _soroswap: &'a SoroswapAdapter,
    pub aquarius: &'a AquariusAdapter,
    pub phoenix: &'a PhoenixAdapter,
    pub comet: &'a CometAdapter,
    pub sushi: &'a SushiAdapter,
    pub aquarius_clmm: &'a AquariusClmmAdapter,
    pub sources: &'a mut Vec<SourceSnapshot>,
    pub clmm_pools: &'a mut Vec<ClmmPoolSnapshot>,
    pub clmm_metrics: Option<&'a ClmmCoverageMetrics>,
}

pub async fn refresh_touched_pools(ctx: TouchedRefreshContext<'_>, touched: HashSet<PoolRef>) -> Result<usize> {
    if touched.is_empty() {
        return Ok(0);
    }

    let max = ledger_max_touched_refresh_from_env();
    let mut pools: Vec<PoolRef> = touched.into_iter().collect();
    pools.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.pool_address.cmp(&b.pool_address))
    });
    pools.truncate(max);

    let mut by_source: HashMap<String, Vec<String>> = HashMap::new();
    for pool in &pools {
        by_source
            .entry(pool.source.clone())
            .or_default()
            .push(pool.pool_address.clone());
    }

    let mut updated = 0usize;
    let mut xyk_writeback: Vec<XykPoolStateValue> = Vec::new();
    let mut aquarius_writeback: Vec<AquariusPoolStateValue> = Vec::new();
    let mut comet_writeback: Vec<CometPoolStateValue> = Vec::new();
    let mut clmm_writeback: Vec<ClmmPoolSnapshot> = Vec::new();

    for (source, addresses) in &by_source {
        if BATCH_XYK_SOURCES.contains(&source.as_str()) {
            if let Some((n, values)) = refresh_xyk_batch(ctx.rpc, ctx.sources, source, addresses).await? {
                updated += n;
                xyk_writeback.extend(values);
            }
        } else if source == AQUARIUS_SOURCE {
            let n = ctx.aquarius.refresh_pool_addresses(addresses).await?;
            if n > 0 {
                updated += n;
                aquarius_writeback.extend(collect_aquarius_writeback(ctx.aquarius, addresses).await);
            }
        } else if source == PHOENIX_SOURCE {
            let n = ctx.phoenix.refresh_touched_pools(addresses).await?;
            if n > 0 {
                merge_xyk_topology_from_adapter(ctx.sources, PHOENIX_SOURCE, ctx.phoenix as &dyn DexAdapter).await;
                updated += n;
                xyk_writeback.extend(
                    collect_xyk_from_adapter(ctx.sources, PHOENIX_SOURCE, addresses, ctx.phoenix as &dyn DexAdapter)
                        .await,
                );
            }
        } else if source == COMET_SOURCE {
            let n = refresh_comet_touched(ctx.comet, ctx.sources, addresses).await?;
            updated += n;
            comet_writeback.extend(collect_comet_writeback(ctx.comet, addresses).await);
        } else if CLMM_SOURCES.contains(&source.as_str()) {
            let (n, snaps) = refresh_clmm_pools(
                source,
                addresses,
                ctx.sushi,
                ctx.aquarius_clmm,
                ctx.clmm_pools,
                ctx.clmm_metrics,
            )
            .await?;
            updated += n;
            clmm_writeback.extend(snaps);
        } else {
            debug!(
                source,
                pools = addresses.len(),
                "Ledger touch: no partial refresh handler"
            );
        }
    }

    if !xyk_writeback.is_empty() {
        ctx.pool_store.set_xyk_batch(&xyk_writeback).await?;
    }
    if !aquarius_writeback.is_empty() {
        ctx.pool_store.set_aquarius_batch(&aquarius_writeback).await?;
    }
    if !comet_writeback.is_empty() {
        ctx.pool_store.set_comet_batch(&comet_writeback).await?;
    }
    if !clmm_writeback.is_empty() {
        ctx.pool_store.set_clmm_batch(&clmm_writeback).await?;
    }

    Ok(updated)
}

async fn refresh_xyk_batch(
    rpc: &SorobanRpc,
    sources: &[SourceSnapshot],
    source: &str,
    pool_addresses: &[String],
) -> Result<Option<(usize, Vec<XykPoolStateValue>)>> {
    if pool_addresses.is_empty() {
        return Ok(None);
    }
    let results = batch_refresh_soroswap_reserves(rpc, pool_addresses).await?;
    let source_snapshot = sources.iter().find(|s| s.source == source);
    let Some(source_snapshot) = source_snapshot else {
        return Ok(None);
    };

    let mut updated = 0usize;
    let mut values = Vec::new();
    for (addr, reserves) in results {
        let Some((r0, r1)) = reserves else {
            continue;
        };
        let Some(pair) = source_snapshot.pairs.iter().find(|p| p.pool_address == addr) else {
            continue;
        };
        values.push(XykPoolStateValue::new(
            source,
            &addr,
            &pair.token_a,
            &pair.token_b,
            pair.fee_bps,
            r0,
            r1,
        ));
        updated += 1;
    }
    Ok(Some((updated, values)))
}

async fn collect_aquarius_writeback(
    aquarius: &AquariusAdapter,
    pool_addresses: &[String],
) -> Vec<AquariusPoolStateValue> {
    aquarius
        .export_pool_quote_states_for(pool_addresses)
        .await
        .into_iter()
        .map(|state| AquariusPoolStateValue {
            pool_address: state.pool_address,
            tokens: state.tokens,
            reserves: state.reserves,
            fee_bps: state.fee_bps,
            is_stable: state.is_stable,
            amp: state.amp,
            updated_at_ms: 0,
        })
        .collect()
}

async fn merge_xyk_topology_from_adapter(sources: &mut [SourceSnapshot], source: &str, adapter: &dyn DexAdapter) {
    let cached = adapter.get_cached_pairs().await;
    let Some(existing) = sources.iter_mut().find(|s| s.source == source) else {
        return;
    };
    for pair in cached {
        if let Some(snap) = existing.pairs.iter_mut().find(|p| p.pool_address == pair.pool_address) {
            snap.fee_bps = pair.fee_bps;
        }
    }
}

async fn collect_xyk_from_adapter(
    sources: &[SourceSnapshot],
    source: &str,
    pool_addresses: &[String],
    adapter: &dyn DexAdapter,
) -> Vec<XykPoolStateValue> {
    let wanted: HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
    let topology: HashMap<String, _> = sources
        .iter()
        .find(|s| s.source == source)
        .into_iter()
        .flat_map(|s| &s.pairs)
        .map(|p| (p.pool_address.clone(), p))
        .collect();

    let mut out = Vec::new();
    for pair in adapter.get_cached_pairs().await {
        if !wanted.contains(pair.pool_address.as_str()) {
            continue;
        }
        let (Some(reserve_a), Some(reserve_b)) = (pair.reserve_a, pair.reserve_b) else {
            continue;
        };
        let Some(topo) = topology.get(&pair.pool_address) else {
            continue;
        };
        out.push(XykPoolStateValue::new(
            source,
            &pair.pool_address,
            &topo.token_a,
            &topo.token_b,
            pair.fee_bps,
            reserve_a,
            reserve_b,
        ));
    }
    out
}

async fn refresh_comet_touched(
    comet: &CometAdapter,
    sources: &mut [SourceSnapshot],
    pool_addresses: &[String],
) -> Result<usize> {
    let mut updated = 0usize;
    for addr in pool_addresses {
        if !comet.refresh_pool(addr).await? {
            continue;
        }
        updated += 1;
        let refreshed: Vec<_> = comet
            .get_cached_pairs()
            .await
            .into_iter()
            .filter(|p| p.pool_address == *addr)
            .collect();
        if let Some(existing) = sources.iter_mut().find(|s| s.source == COMET_SOURCE) {
            for pair in refreshed {
                if let Some(snap) = existing.pairs.iter_mut().find(|p| {
                    p.pool_address == pair.pool_address
                        && p.token_a == pair.token_a.canonical()
                        && p.token_b == pair.token_b.canonical()
                }) {
                    snap.fee_bps = pair.fee_bps;
                }
            }
        }
    }
    Ok(updated)
}

async fn collect_comet_writeback(comet: &CometAdapter, pool_addresses: &[String]) -> Vec<CometPoolStateValue> {
    comet
        .export_pool_quote_states_for(pool_addresses)
        .await
        .into_iter()
        .filter(|(_, state)| state.records.len() >= 2)
        .map(|(addr, state)| comet_state_to_value(&addr, &state))
        .collect()
}

async fn refresh_clmm_pools(
    source: &str,
    pool_addresses: &[String],
    sushi: &SushiAdapter,
    aquarius_clmm: &AquariusClmmAdapter,
    clmm_pools: &mut Vec<ClmmPoolSnapshot>,
    clmm_metrics: Option<&ClmmCoverageMetrics>,
) -> Result<(usize, Vec<ClmmPoolSnapshot>)> {
    let mut updated = 0usize;
    let mut snapshots = Vec::new();

    for addr in pool_addresses {
        let result = match source {
            "sushi" => sushi.ensure_pool_loaded(addr).await,
            "aquarius_clmm" => aquarius_clmm.ensure_pool_loaded(addr).await,
            _ => continue,
        };
        if let Err(error) = result {
            warn!(source, pool = %addr, %error, "CLMM touched refresh failed");
            continue;
        }
        updated += 1;
    }

    let exported = match source {
        "sushi" => sushi.export_clmm_snapshots().await,
        "aquarius_clmm" => aquarius_clmm.export_clmm_snapshots().await,
        _ => Vec::new(),
    };

    let wanted: HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
    for snap in exported {
        if !wanted.contains(snap.pool_address.as_str()) {
            continue;
        }
        if let Some(metrics) = clmm_metrics {
            metrics.record_snapshot(&snap);
        }
        if !should_publish_clmm_to_redis(&snap) {
            debug!(
                source,
                pool = %snap.pool_address,
                "CLMM touched refresh: skipped Redis publish (incomplete coverage)"
            );
            continue;
        }
        if let Some(existing) = clmm_pools
            .iter_mut()
            .find(|p| p.source == snap.source && p.pool_address == snap.pool_address)
        {
            *existing = snap.clone();
        } else {
            clmm_pools.push(snap.clone());
        }
        snapshots.push(snap);
    }

    Ok((updated, snapshots))
}

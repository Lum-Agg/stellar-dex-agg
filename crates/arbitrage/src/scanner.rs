//! Scan configured base/bridge pairs for profitable round-trip swaps.

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        context::ArbContext,
        dedup::round_trip_dedup_key,
        execute::try_execute_opportunity,
        hydrate::hydrate_paths,
        optimize::optimize_round_trip,
        runtime::ArbRuntime,
        vault::resolve_max_amount_in,
    },
    anyhow::Result,
    router_engine::{Path, RouteRequest},
    std::sync::atomic::Ordering,
    tracing::{info, warn},
};

#[derive(Debug, Clone)]
pub struct ArbOpportunity {
    pub quote: RoundTripQuote,
    pub profit_bps: i64,
    pub route_label: String,
}

pub fn compute_profit_bps(amount_in: u128, amount_out: u128) -> i64 {
    if amount_in == 0 {
        return 0;
    }
    let ain = amount_in as i64;
    let aout = amount_out as i64;
    (aout - ain) * 10_000 / ain
}

/// Collect candidate paths for all configured bridges (used by scanner + diag).
pub async fn collect_paths_for_base(ctx: &ArbContext, base: &router_engine::TokenId) -> Vec<Path> {
    let mut paths = Vec::new();
    let amount = ctx.config.probe_amount_in;

    for bridge in &ctx.config.bridge_tokens {
        if bridge.canonical() == base.canonical() {
            continue;
        }

        for (token_in, token_out) in [(base, bridge), (bridge, base)] {
            let request = RouteRequest {
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                amount_in: amount,
                slippage_bps: if ctx.config.slippage_bps == 0 {
                    None
                } else {
                    Some(ctx.config.slippage_bps)
                },
                max_hops: Some(ctx.config.max_hops),
                max_splits: Some(ctx.config.max_splits),
                prefer_soroban: Some(true),
            };
            paths.extend(ctx.engine.find_candidate_paths(&request).await);
        }
    }

    paths
}

pub async fn scan_once(runtime: &ArbRuntime) -> Result<Vec<ArbOpportunity>> {
    let ctx = runtime.connect().await?;
    scan_with_context(runtime, &ctx).await
}

async fn scan_with_context(runtime: &ArbRuntime, ctx: &ArbContext) -> Result<Vec<ArbOpportunity>> {
    let try_execute = runtime.build_enabled();

    info!(
        bases = ctx.config.base_tokens.len(),
        bridges = ctx.config.bridge_tokens.len(),
        snapshot_version = %ctx.snapshot.version,
        build_tx = try_execute,
        submit_tx = runtime.submit_enabled(),
        callers = runtime.caller_pool.as_ref().map(|p| p.len()).unwrap_or(0),
        aggregator = ?ctx.config.aggregator_contract,
        "round-trip arb scan starting"
    );

    let mut opportunities = Vec::new();

    for base in &ctx.config.base_tokens {
        let candidate_paths = collect_paths_for_base(ctx, base).await;
        let (hydration, redis_miss) = hydrate_paths(&candidate_paths, ctx.pool_store.as_ref()).await;
        if redis_miss > 0 {
            warn!(
                base = %base.canonical(),
                redis_miss_xyk = redis_miss,
                paths = candidate_paths.len(),
                "quote hydration incomplete"
            );
        }

        let mut quoted = 0usize;
        let max_in = resolve_max_amount_in(ctx, &base.canonical()).await;
        if max_in < ctx.config.min_amount_in {
            warn!(
                base = %base.canonical(),
                max_in,
                min_amount_in = ctx.config.min_amount_in,
                vault_balance = ?ctx.vault_base_balance,
                "skipping base — vault float below min trade size"
            );
            continue;
        }

        for bridge in &ctx.config.bridge_tokens {
            if bridge.canonical() == base.canonical() {
                continue;
            }

            let Some(probe) = quote_round_trip(ctx, base, bridge, ctx.config.probe_amount_in, &hydration).await else {
                continue;
            };
            quoted += 1;

            let quote = if ctx.config.optimize_amount {
                optimize_round_trip(
                    ctx,
                    base,
                    bridge,
                    &hydration,
                    ctx.config.min_amount_in,
                    max_in,
                    ctx.config.sample_count,
                )
                .await
                .unwrap_or(probe)
            } else {
                probe
            };

            let profit = quote.profit();
            let profit_bps = compute_profit_bps(quote.amount_in, quote.amount_out);
            if profit < ctx.config.min_profit {
                continue;
            }

            runtime.stats.opportunities.fetch_add(1, Ordering::Relaxed);

            let route_label = quote.route_label();
            info!(
                base = %base.canonical(),
                bridge = %bridge.canonical(),
                profit,
                profit_bps,
                amount_in = quote.amount_in,
                amount_out = quote.amount_out,
                leg_out_splits = quote.leg_out.sub_orders.len(),
                leg_back_splits = quote.leg_back.sub_orders.len(),
                route = %route_label,
                "round-trip opportunity"
            );

            let opp = ArbOpportunity {
                quote: quote.clone(),
                profit_bps,
                route_label,
            };

            if try_execute {
                if let Some(pool) = &runtime.caller_pool {
                    let path_key = round_trip_dedup_key(base, bridge);
                    if runtime.submit_enabled() {
                        let mut cache = runtime.path_cache.lock().await;
                        if cache.recently_submitted(&path_key) {
                            runtime.stats.txs_dedup_skipped.fetch_add(1, Ordering::Relaxed);
                            opportunities.push(opp);
                            continue;
                        }
                        cache.mark_submitted(path_key);
                    }

                    if let Err(e) =
                        try_execute_opportunity(ctx, &opp, &hydration, pool, &runtime.stats, runtime.dry_run()).await
                    {
                        warn!(route = %opp.route_label, error = %e, "round_trip_swap pipeline failed");
                    }
                }
            }

            opportunities.push(opp);
        }

        info!(
            base = %base.canonical(),
            bridges = ctx.config.bridge_tokens.len(),
            quoted,
            opportunities = opportunities.len(),
            "round-trip scan complete for hub"
        );
    }

    Ok(opportunities)
}

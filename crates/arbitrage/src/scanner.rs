//! Scan configured base/bridge pairs for profitable round-trip swaps.

use {
    crate::{
        bridge::quote_round_trip, context::ArbContext, optimize::optimize_round_trip, runtime::ArbRuntime,
        stats::ArbStats, vault::resolve_max_amount_in,
    },
    anyhow::Result,
    router_engine::TokenId,
    std::sync::atomic::Ordering,
    tracing::{debug, info, warn},
};

#[derive(Debug, Clone)]
pub struct ArbOpportunity {
    pub quote: crate::bridge::RoundTripQuote,
    pub profit_bps: i64,
    pub route_label: String,
}

pub fn compute_profit_bps(amount_in: u128, amount_out: u128) -> i64 {
    if amount_in == 0 {
        return 0;
    }
    let (negative, delta) = if amount_out >= amount_in {
        (false, amount_out - amount_in)
    } else {
        (true, amount_in - amount_out)
    };
    let magnitude = delta.saturating_mul(10_000) / amount_in;
    let magnitude = i64::try_from(magnitude).unwrap_or(i64::MAX);
    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// Quote + size one base×bridge pair; used by burberry workers and legacy
/// scan_once.
pub async fn evaluate_bridge_pair(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    stats: &ArbStats,
) -> Result<Option<ArbOpportunity>> {
    let max_in = resolve_max_amount_in(ctx, &base.canonical()).await;
    if max_in < ctx.config.min_amount_in {
        return Ok(None);
    }

    let probe = match quote_round_trip(ctx, base, bridge, ctx.config.probe_amount_in).await {
        Ok(probe) => probe,
        Err(error) => {
            stats.quote_failed.fetch_add(1, Ordering::Relaxed);
            debug!(
                base = %base.canonical(),
                bridge = %bridge.canonical(),
                error = %error,
                "round-trip quote failed"
            );
            return Ok(None);
        }
    };

    // Always size-search when enabled — small probe can be flat while a larger
    // size clears ARB_MIN_PROFIT (still gated below after optimize).
    let quote = if ctx.config.optimize_amount {
        optimize_round_trip(
            ctx,
            base,
            bridge,
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
    if profit < ctx.config.min_profit_for(&base.canonical()) {
        stats.unprofitable_quotes.fetch_add(1, Ordering::Relaxed);
        return Ok(None);
    }

    stats.opportunities.fetch_add(1, Ordering::Relaxed);

    let route_label = quote.route_label();
    info!(
        base = %base.canonical(),
        bridge = %bridge.canonical(),
        profit,
        profit_bps,
        amount_in = quote.amount_in,
        amount_out = quote.amount_out,
        minimum_out = quote.minimum_out,
        leg_out_splits = quote.leg_out.route.sub_orders.len(),
        leg_back_splits = quote.leg_back.route.sub_orders.len(),
        route = %route_label,
        "round-trip opportunity"
    );

    Ok(Some(ArbOpportunity {
        quote,
        profit_bps,
        route_label,
    }))
}

/// Legacy sequential full scan (tests / one-shot).
pub async fn scan_once(runtime: &ArbRuntime) -> Result<Vec<ArbOpportunity>> {
    let ctx = runtime.connect().await?;
    scan_with_context(runtime, &ctx).await
}

async fn scan_with_context(runtime: &ArbRuntime, ctx: &ArbContext) -> Result<Vec<ArbOpportunity>> {
    info!(
        bases = ctx.config.base_tokens.len(),
        bridges = ctx.config.bridge_tokens.len(),
        quote_api_instances = ctx.config.quote_api_urls.len(),
        quote_api = ?ctx.config.quote_api_urls,
        build_tx = runtime.build_enabled(),
        submit_tx = runtime.submit_enabled(),
        callers = runtime.caller_pool.as_ref().map(|p| p.len()).unwrap_or(0),
        aggregator = ?ctx.config.aggregator_contract,
        "round-trip arb scan starting"
    );

    let mut opportunities = Vec::new();

    for base in &ctx.config.base_tokens {
        let mut quoted = 0usize;
        let max_in = resolve_max_amount_in(ctx, &base.canonical()).await;
        if max_in < ctx.config.min_amount_in {
            warn!(
                base = %base.canonical(),
                max_in,
                min_amount_in = ctx.config.min_amount_in,
                "skipping base — max_amount_in below min trade size"
            );
            continue;
        }

        for bridge in &ctx.config.bridge_tokens {
            if bridge.canonical() == base.canonical() {
                continue;
            }
            quoted += 1;
            runtime.stats.routes_evaluated.fetch_add(1, Ordering::Relaxed);

            let Some(opp) = evaluate_bridge_pair(ctx, base, bridge, &runtime.stats).await? else {
                continue;
            };

            if runtime.build_enabled() {
                if let Some(pool) = &runtime.caller_pool {
                    if let Err(e) = crate::execute::try_execute_opportunity(
                        ctx,
                        &opp,
                        pool,
                        runtime.stats.clone(),
                        runtime.profit.clone(),
                        runtime.dry_run(),
                    )
                    .await
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

#[cfg(test)]
mod tests {
    use super::compute_profit_bps;

    #[test]
    fn computes_profit_bps_without_narrowing_amounts() {
        let amount_in = (i64::MAX as u128 + 1) * 100;
        assert_eq!(compute_profit_bps(amount_in, amount_in + amount_in / 100), 100);
        assert_eq!(compute_profit_bps(amount_in, amount_in - amount_in / 100), -100);
    }
}

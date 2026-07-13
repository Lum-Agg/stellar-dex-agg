//! Two-leg round-trip quotes: base → bridge → base via aggregator routing.

use {
    crate::context::ArbContext,
    anyhow::Result,
    router_engine::{OptimalRoute, Path, QuoteHydration, RouteRequest, TokenId},
};

/// Quote one directed leg (may include split sub-orders).
pub async fn quote_leg(
    ctx: &ArbContext,
    token_in: &TokenId,
    token_out: &TokenId,
    amount_in: u128,
    hydration: &QuoteHydration,
) -> Option<OptimalRoute> {
    if amount_in == 0 {
        return None;
    }

    let request = RouteRequest {
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        amount_in,
        slippage_bps: if ctx.config.slippage_bps == 0 {
            None
        } else {
            Some(ctx.config.slippage_bps)
        },
        max_hops: Some(ctx.config.max_hops),
        max_splits: Some(ctx.config.max_splits),
        prefer_soroban: Some(true),
    };

    let paths = ctx.engine.find_candidate_paths(&request).await;
    if paths.is_empty() {
        return None;
    }

    let route = ctx.engine.get_route_with_paths(&request, &paths, Some(hydration)).await;

    if route.total_expected_out == 0 || route.sub_orders.is_empty() {
        return None;
    }

    Some(route)
}

/// Quote strict two-leg round trip at a fixed base input.
///
/// When `leg_out` is split, quote each split's bridge output back to base
/// separately so on-chain `leg_back` amounts match `leg_out` outputs.
pub async fn quote_round_trip(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    amount_in: u128,
    hydration: &QuoteHydration,
) -> Option<RoundTripQuote> {
    let leg_out = quote_leg(ctx, base, bridge, amount_in, hydration).await?;

    let leg_back = if leg_out.sub_orders.len() <= 1 {
        quote_leg(ctx, bridge, base, leg_out.total_expected_out, hydration).await?
    } else {
        let mut back_subs = Vec::new();
        let mut total_back_out = 0u128;
        for sub in &leg_out.sub_orders {
            let bridge_in = sub.expected_amount_out;
            if bridge_in == 0 {
                return None;
            }
            let partial = quote_leg(ctx, bridge, base, bridge_in, hydration).await?;
            total_back_out = total_back_out.saturating_add(partial.total_expected_out);
            back_subs.extend(partial.sub_orders);
        }
        router_engine::OptimalRoute {
            sub_orders: back_subs,
            total_amount_in: leg_out.total_expected_out,
            total_expected_out: total_back_out,
            price_impact_bps: 0,
            is_split: leg_out.sub_orders.len() > 1,
            improvement_bps: 0,
            minimum_out: 0,
            compute_time_ms: 0,
            debug: None,
        }
    };

    let amount_out = leg_back.total_expected_out;

    Some(RoundTripQuote {
        base: base.clone(),
        bridge: bridge.clone(),
        amount_in,
        amount_out,
        leg_out,
        leg_back,
    })
}

#[derive(Debug, Clone)]
pub struct RoundTripQuote {
    pub base: TokenId,
    pub bridge: TokenId,
    pub amount_in: u128,
    pub amount_out: u128,
    pub leg_out: OptimalRoute,
    pub leg_back: OptimalRoute,
}

impl RoundTripQuote {
    pub fn profit(&self) -> u128 {
        self.amount_out.saturating_sub(self.amount_in)
    }

    pub fn route_label(&self) -> String {
        format!(
            "{} → {} → {}",
            self.base.canonical(),
            self.bridge.canonical(),
            self.base.canonical()
        )
    }

    pub fn all_paths(&self) -> Vec<Path> {
        let mut paths = Vec::new();
        for sub in &self.leg_out.sub_orders {
            paths.push(sub.path.clone());
        }
        for sub in &self.leg_back.sub_orders {
            paths.push(sub.path.clone());
        }
        paths
    }
}

pub fn paths_for_hydration(quotes: &[RoundTripQuote]) -> Vec<Path> {
    let mut paths = Vec::new();
    for q in quotes {
        paths.extend(q.all_paths());
    }
    paths
}

pub async fn hydrate_for_quotes(ctx: &ArbContext, quotes: &[RoundTripQuote]) -> Result<QuoteHydration> {
    let paths = paths_for_hydration(quotes);
    let (hydration, _) = crate::hydrate::hydrate_paths(&paths, ctx.pool_store.as_ref()).await;
    Ok(hydration)
}

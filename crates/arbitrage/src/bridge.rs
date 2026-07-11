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
        slippage_bps: Some(ctx.config.slippage_bps),
        max_hops: Some(ctx.config.max_hops),
        max_splits: Some(ctx.config.max_splits),
        prefer_soroban: None,
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
pub async fn quote_round_trip(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    amount_in: u128,
    hydration: &QuoteHydration,
) -> Option<RoundTripQuote> {
    let leg_out = quote_leg(ctx, base, bridge, amount_in, hydration).await?;
    let bridge_amount = leg_out.total_expected_out;
    let leg_back = quote_leg(ctx, bridge, base, bridge_amount, hydration).await?;
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

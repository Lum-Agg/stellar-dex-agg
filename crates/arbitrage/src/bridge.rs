//! Two-leg round-trip quotes via LumAgg quote-api.

use {
    crate::{context::ArbContext, quote_client::LegQuote},
    anyhow::Result,
    router_engine::{OptimalRoute, TokenId},
};

/// Quote one directed leg (may include split sub-orders).
pub async fn quote_leg(ctx: &ArbContext, token_in: &TokenId, token_out: &TokenId, amount_in: u128) -> Result<LegQuote> {
    ctx.quote_client
        .quote_leg(&ctx.config, token_in, token_out, amount_in)
        .await
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
) -> Result<RoundTripQuote> {
    quote_round_trip_with_validation(ctx, base, bridge, amount_in, ctx.config.on_chain_validate).await
}

/// Quote a round trip with an explicit validation mode. The scanner normally
/// uses local quote math; execution validates only the selected opportunity.
pub async fn quote_round_trip_with_validation(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    amount_in: u128,
    on_chain_validate: bool,
) -> Result<RoundTripQuote> {
    let leg_out = ctx
        .quote_client
        .quote_leg_with_validation(&ctx.config, base, bridge, amount_in, on_chain_validate)
        .await?;

    let leg_back = if leg_out.route.sub_orders.len() <= 1 {
        ctx.quote_client
            .quote_leg_with_validation(
                &ctx.config,
                bridge,
                base,
                leg_out.route.total_expected_out,
                on_chain_validate,
            )
            .await?
    } else {
        let mut back_subs = Vec::new();
        let mut back_steps = Vec::new();
        let mut total_back_out = 0u128;
        let mut total_minimum_out = 0u128;
        for (sub, _steps) in leg_out.route.sub_orders.iter().zip(leg_out.step_sets.iter()) {
            let bridge_in = sub.expected_amount_out;
            if bridge_in == 0 {
                return Err(anyhow::anyhow!("leg_out split produced zero bridge output"));
            }
            let partial = ctx
                .quote_client
                .quote_leg_with_validation(&ctx.config, bridge, base, bridge_in, on_chain_validate)
                .await?;
            total_back_out = total_back_out.saturating_add(partial.route.total_expected_out);
            total_minimum_out = total_minimum_out.saturating_add(partial.minimum_out);
            back_subs.extend(partial.route.sub_orders);
            back_steps.extend(partial.step_sets);
        }
        LegQuote {
            route: OptimalRoute {
                sub_orders: back_subs,
                total_amount_in: leg_out.route.total_expected_out,
                total_expected_out: total_back_out,
                price_impact_bps: 0,
                is_split: leg_out.route.sub_orders.len() > 1,
                improvement_bps: 0,
                minimum_out: total_minimum_out,
                compute_time_ms: 0,
                debug: None,
            },
            step_sets: back_steps,
            minimum_out: total_minimum_out,
        }
    };

    let amount_out = leg_back.route.total_expected_out;
    let minimum_out = leg_back.minimum_out;

    Ok(RoundTripQuote {
        base: base.clone(),
        bridge: bridge.clone(),
        amount_in,
        amount_out,
        minimum_out,
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
    /// Chain floor from quote-api leg_back (`minimum_output`).
    pub minimum_out: u128,
    pub leg_out: LegQuote,
    pub leg_back: LegQuote,
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
}

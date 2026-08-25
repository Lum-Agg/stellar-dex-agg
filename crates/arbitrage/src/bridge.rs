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
            snapshot_age_ms: leg_out.snapshot_age_ms,
            pool_state_age_ms: leg_out.pool_state_age_ms,
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

    pub fn venue_route_label(&self) -> String {
        fn leg_label(leg: &LegQuote) -> String {
            let mut venues = Vec::new();
            for steps in &leg.step_sets {
                for step in steps {
                    if !venues.iter().any(|venue| venue == &step.dex_type) {
                        venues.push(step.dex_type.clone());
                    }
                }
            }
            if venues.is_empty() {
                "unknown".into()
            } else {
                venues.join("+")
            }
        }

        format!("{} → {}", leg_label(&self.leg_out), leg_label(&self.leg_back))
    }

    /// Venue and pool identity for post-submit diagnostics. Split sub-routes
    /// are separated with `||` so the full route remains unambiguous.
    pub fn pool_route_label(&self) -> String {
        fn leg_label(leg: &LegQuote) -> String {
            let routes: Vec<String> = leg
                .step_sets
                .iter()
                .map(|steps| {
                    steps
                        .iter()
                        .map(|step| format!("{}:{}", step.venue_type, step.pool_address))
                        .collect::<Vec<_>>()
                        .join(" → ")
                })
                .collect();
            if routes.is_empty() {
                "unknown".into()
            } else {
                routes.join(" || ")
            }
        }

        format!("{} || {}", leg_label(&self.leg_out), leg_label(&self.leg_back))
    }

    pub fn quote_snapshot_age_ms(&self) -> Option<u64> {
        max_age(self.leg_out.snapshot_age_ms, self.leg_back.snapshot_age_ms)
    }

    pub fn pool_state_age_ms(&self) -> Option<u64> {
        max_age(self.leg_out.pool_state_age_ms, self.leg_back.pool_state_age_ms)
    }
}

fn max_age(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    left.into_iter().chain(right).max()
}

#[cfg(test)]
mod tests {
    use {super::*, crate::invoke::ArbSwapStep};

    fn leg_with_step(venue_type: &str, dex_type: &str, pool_address: &str) -> LegQuote {
        LegQuote {
            route: OptimalRoute {
                sub_orders: vec![],
                total_amount_in: 0,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                minimum_out: 0,
                compute_time_ms: 0,
                debug: None,
            },
            step_sets: vec![vec![ArbSwapStep {
                venue_type: venue_type.into(),
                dex_type: dex_type.into(),
                pool_address: pool_address.into(),
                token_in: "A".into(),
                token_out: "B".into(),
                in_idx: 0,
                out_idx: 1,
            }]],
            minimum_out: 0,
            snapshot_age_ms: None,
            pool_state_age_ms: None,
        }
    }

    #[test]
    fn pool_route_preserves_clmm_venue_label() {
        let quote = RoundTripQuote {
            base: TokenId::from_str_auto("A"),
            bridge: TokenId::from_str_auto("B"),
            amount_in: 0,
            amount_out: 0,
            minimum_out: 0,
            leg_out: leg_with_step("aquarius_clmm", "aquarius", "CLMM_POOL"),
            leg_back: leg_with_step("aquarius", "aquarius", "XYK_POOL"),
        };

        assert_eq!(quote.pool_route_label(), "aquarius_clmm:CLMM_POOL || aquarius:XYK_POOL");
    }
}

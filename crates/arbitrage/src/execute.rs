//! Build + optionally submit `aggregator.round_trip_swap` transactions.

use {
    crate::{
        bridge::{quote_leg, quote_round_trip, RoundTripQuote},
        callers::CallerPool,
        config::ArbConfig,
        context::ArbContext,
        invoke::{
            build_execute_round_trip_op, build_raw_envelope_xdr, build_round_trip_swap_op, min_amount_out_break_even,
        },
        optimize::optimize_round_trip,
        prepare::{
            fetch_account_sequence, parse_base_received_from_sim_error, parse_bridge_received_from_sim_error,
            prepare_transaction_xdr,
        },
        scanner::ArbOpportunity,
        stats::ArbStats,
        submit::submit_prepared,
        vault::resolve_max_amount_in,
    },
    anyhow::{Context, Result},
    stellar_xdr::curr as sxdr,
    tracing::{info, warn},
};

#[derive(Debug, Clone)]
pub struct PreparedArbTx {
    pub route_label: String,
    pub caller_public_key: String,
    pub amount_in: u128,
    /// Off-chain quoted output (quote-api).
    pub quoted_amount_out: u128,
    /// On-chain simulated output (`base_total` from contract return).
    pub simulated_amount_out: u128,
    /// Inclusion + resource fee from simulate (stroops).
    pub estimated_fee_stroops: u128,
    pub profit_bps: i64,
    pub unsigned_tx_xdr: String,
    pub simulated: bool,
}

async fn resolve_quote(ctx: &ArbContext, opp: &ArbOpportunity) -> Option<RoundTripQuote> {
    let max_in = resolve_max_amount_in(ctx, &opp.quote.base.canonical()).await;
    if max_in < ctx.config.min_amount_in {
        return None;
    }
    if ctx.config.optimize_amount {
        optimize_round_trip(
            ctx,
            &opp.quote.base,
            &opp.quote.bridge,
            ctx.config.min_amount_in,
            max_in,
            ctx.config.sample_count,
        )
        .await
    } else {
        quote_round_trip(ctx, &opp.quote.base, &opp.quote.bridge, opp.quote.amount_in.min(max_in))
            .await
            .ok()
    }
}

fn build_op_for_quote(
    ctx: &ArbContext,
    aggregator: &str,
    caller_public_key: &str,
    quote: &RoundTripQuote,
    min_amount_out: i128,
    bridge_amount_override: Option<u128>,
) -> Result<sxdr::Operation> {
    let amount_in_i128 = i128::try_from(quote.amount_in).context("amount_in exceeds i128")?;
    if let Some(vault) = ctx.config.vault_contract.as_deref() {
        build_execute_round_trip_op(
            vault,
            aggregator,
            caller_public_key,
            &quote.base.canonical(),
            &quote.bridge.canonical(),
            amount_in_i128,
            &quote.leg_out,
            &quote.leg_back,
            min_amount_out,
            bridge_amount_override,
        )
    } else {
        build_round_trip_swap_op(
            aggregator,
            caller_public_key,
            &quote.base.canonical(),
            &quote.bridge.canonical(),
            amount_in_i128,
            &quote.leg_out,
            &quote.leg_back,
            min_amount_out,
            bridge_amount_override,
        )
    }
}

/// Re-quote leg_back at the on-chain bridge amount from leg_out.
async fn refresh_leg_back_at_bridge(ctx: &ArbContext, quote: &mut RoundTripQuote, bridge_total: u128) -> Result<()> {
    if quote.leg_out.route.sub_orders.len() <= 1 {
        quote.leg_back = quote_leg(ctx, &quote.bridge, &quote.base, bridge_total).await?;
    } else {
        let mut back_subs = Vec::new();
        let mut back_steps = Vec::new();
        let mut total_back_out = 0u128;
        let mut total_minimum_out = 0u128;
        for sub in &quote.leg_out.route.sub_orders {
            let bridge_in = sub.expected_amount_out;
            if bridge_in == 0 {
                return Err(anyhow::anyhow!("leg_out split produced zero bridge output"));
            }
            let partial = quote_leg(ctx, &quote.bridge, &quote.base, bridge_in).await?;
            total_back_out = total_back_out.saturating_add(partial.route.total_expected_out);
            total_minimum_out = total_minimum_out.saturating_add(partial.minimum_out);
            back_subs.extend(partial.route.sub_orders);
            back_steps.extend(partial.step_sets);
        }
        quote.leg_back = crate::quote_client::LegQuote {
            route: router_engine::OptimalRoute {
                sub_orders: back_subs,
                total_amount_in: bridge_total,
                total_expected_out: total_back_out,
                price_impact_bps: 0,
                is_split: quote.leg_out.route.sub_orders.len() > 1,
                improvement_bps: 0,
                minimum_out: total_minimum_out,
                compute_time_ms: 0,
                debug: None,
            },
            step_sets: back_steps,
            minimum_out: total_minimum_out,
        };
    }
    quote.amount_out = quote.leg_back.route.total_expected_out;
    quote.minimum_out = quote.leg_back.minimum_out;
    Ok(())
}

pub async fn prepare_opportunity_tx(
    ctx: &ArbContext,
    opp: &ArbOpportunity,
    caller_public_key: &str,
    stats: &ArbStats,
) -> Result<Option<PreparedArbTx>> {
    let Some(aggregator) = ctx.config.aggregator_contract.as_deref() else {
        return Ok(None);
    };

    let mut quote = match resolve_quote(ctx, opp).await {
        Some(q) => q,
        None => return Ok(None),
    };

    if quote.profit() < ctx.config.min_profit {
        return Ok(None);
    }

    let seq = fetch_account_sequence(&ctx.config.rpc_url, caller_public_key).await?;
    let fee = 100_000u32;
    let mut bridge_override = None;
    let mut unsigned_tx_xdr = String::new();
    let mut simulated = false;
    let mut simulated_amount_out = 0u128;
    let mut estimated_fee_stroops = 0u128;
    let mut tried_bridge_fix = false;
    let mut tried_probe_fallback = false;

    // Up to 3 sims: initial → bridge-corrected → optional probe-size fallback.
    for _attempt in 0..3 {
        let min_amount_out = min_amount_out_break_even(quote.amount_in);
        let op = build_op_for_quote(
            ctx,
            aggregator,
            caller_public_key,
            &quote,
            min_amount_out,
            bridge_override,
        )?;

        match prepare_transaction_xdr(
            &ctx.config.rpc_url,
            caller_public_key,
            seq as u64,
            std::slice::from_ref(&op),
            fee,
        )
        .await
        {
            Ok(prepared) => {
                unsigned_tx_xdr = prepared.unsigned_tx_xdr;
                simulated = true;
                simulated_amount_out = prepared.amount_out;
                estimated_fee_stroops = prepared.estimated_fee_stroops;
                break;
            }
            Err(err) => {
                let err_str = err.to_string();

                // 1) Align leg_back to on-chain leg_out bridge amount (once).
                if !tried_bridge_fix {
                    if let Some(bridge_total) =
                        parse_bridge_received_from_sim_error(&err_str, &quote.bridge.canonical(), aggregator)
                    {
                        if bridge_total > 0 && bridge_total != quote.leg_out.route.total_expected_out {
                            info!(
                                route = %quote.route_label(),
                                quoted_bridge = quote.leg_out.route.total_expected_out,
                                simulated_bridge = bridge_total,
                                "leg_out quote vs simulate mismatch — re-quote leg_back"
                            );
                            tried_bridge_fix = true;
                            refresh_leg_back_at_bridge(ctx, &mut quote, bridge_total).await?;
                            bridge_override = Some(bridge_total);
                            continue;
                        }
                    }
                }

                // 2) Legs completed but break-even assert failed → quote-api phantom profit.
                //    Fall back to probe size so small real opportunities are not skipped.
                if !tried_probe_fallback && quote.amount_in > ctx.config.min_amount_in {
                    if let Some(base_out) = parse_base_received_from_sim_error(
                        &err_str,
                        &quote.base.canonical(),
                        aggregator,
                        caller_public_key,
                    ) {
                        let on_chain_profit = base_out.saturating_sub(quote.amount_in);
                        warn!(
                            route = %quote.route_label(),
                            amount_in = quote.amount_in,
                            quoted_amount_out = quote.amount_out,
                            on_chain_base_out = base_out,
                            on_chain_profit,
                            "optimized size unprofitable on-chain — retry at probe size"
                        );
                        tried_probe_fallback = true;
                        match quote_round_trip(ctx, &quote.base, &quote.bridge, ctx.config.min_amount_in).await {
                            Ok(probe) if probe.profit() >= ctx.config.min_profit => {
                                quote = probe;
                                bridge_override = None;
                                tried_bridge_fix = false;
                                continue;
                            }
                            Ok(_) => {
                                stats
                                    .txs_sim_profit_rejected
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                return Ok(None);
                            }
                            Err(e) => {
                                warn!(error = %e, "probe-size re-quote failed");
                            }
                        }
                    }
                }

                warn!(
                    error = %err,
                    route = %opp.route_label,
                    caller = %caller_public_key,
                    "Soroban prepare failed"
                );
                unsigned_tx_xdr = build_raw_envelope_xdr(caller_public_key, seq as u64, op)?;
                break;
            }
        }
    }

    if simulated {
        let sim_profit = simulated_amount_out.saturating_sub(quote.amount_in);
        // Net of Soroban resource fee — multi-hop/split routes often cost ~0.1 XLM.
        let net_profit = sim_profit.saturating_sub(estimated_fee_stroops);
        if net_profit < ctx.config.min_profit {
            warn!(
                route = %quote.route_label(),
                caller = %caller_public_key,
                amount_in = quote.amount_in,
                quoted_amount_out = quote.amount_out,
                simulated_amount_out,
                simulated_profit = sim_profit,
                estimated_fee_stroops,
                net_profit,
                min_profit = ctx.config.min_profit,
                "simulated net profit below min_profit after fees — discard"
            );
            stats
                .txs_sim_profit_rejected
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(None);
        }
    }

    let profit_bps = crate::scanner::compute_profit_bps(quote.amount_in, simulated_amount_out);
    let simulated_profit = simulated_amount_out.saturating_sub(quote.amount_in);
    let net_profit = simulated_profit.saturating_sub(estimated_fee_stroops);
    let min_amount_out = min_amount_out_break_even(quote.amount_in);

    let tx_kind = if ctx.config.vault_contract.is_some() {
        "vault.execute_round_trip"
    } else {
        "aggregator.round_trip_swap"
    };
    info!(
        route = %quote.route_label(),
        caller = %caller_public_key,
        amount_in = quote.amount_in,
        quoted_amount_out = quote.amount_out,
        quoted_minimum_out = quote.minimum_out,
        simulated_amount_out,
        simulated_profit,
        estimated_fee_stroops,
        net_profit,
        profit_bps,
        simulated,
        min_profit = ctx.config.min_profit,
        min_amount_out,
        leg_out_splits = quote.leg_out.route.sub_orders.len(),
        leg_back_splits = quote.leg_back.route.sub_orders.len(),
        tx_kind,
        "prepared arb tx"
    );

    Ok(Some(PreparedArbTx {
        route_label: quote.route_label(),
        caller_public_key: caller_public_key.to_string(),
        amount_in: quote.amount_in,
        quoted_amount_out: quote.amount_out,
        simulated_amount_out,
        estimated_fee_stroops,
        profit_bps,
        unsigned_tx_xdr,
        simulated,
    }))
}

pub async fn try_execute_opportunity(
    ctx: &ArbContext,
    opp: &ArbOpportunity,
    caller_pool: &CallerPool,
    stats: &ArbStats,
    profit: &crate::profit::ProfitBook,
    dry_run: bool,
) -> Result<()> {
    let Some(guard) = caller_pool.try_acquire().await else {
        warn!(route = %opp.route_label, "all callers busy, dropping opportunity");
        return Ok(());
    };

    let Some(prepared) = prepare_opportunity_tx(ctx, opp, &guard.public_key(), stats).await? else {
        return Ok(());
    };

    stats.txs_prepared.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if !prepared.simulated {
        stats
            .txs_sim_rejected
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        warn!(route = %prepared.route_label, "simulation missing — skip submit");
        return Ok(());
    }

    if dry_run {
        stats.txs_dry_run.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let simulated_profit = prepared.simulated_amount_out.saturating_sub(prepared.amount_in);
        info!(
            route = %prepared.route_label,
            caller = %prepared.caller_public_key,
            simulated_amount_out = prepared.simulated_amount_out,
            simulated_profit,
            estimated_fee_stroops = prepared.estimated_fee_stroops,
            net_profit = simulated_profit.saturating_sub(prepared.estimated_fee_stroops),
            profit_bps = prepared.profit_bps,
            "DRY_RUN: would submit round_trip_swap"
        );
        return Ok(());
    }

    submit_prepared(&ctx.config.rpc_url, guard.keypair(), &prepared, stats, profit).await?;
    Ok(())
}

pub fn execution_enabled(config: &ArbConfig) -> bool {
    config.build_tx && config.aggregator_contract.is_some()
}

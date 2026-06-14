//! Build + optionally submit `aggregator.round_trip_swap` transactions.

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        callers::CallerPool,
        config::ArbConfig,
        context::ArbContext,
        invoke::{build_raw_envelope_xdr, build_round_trip_swap_op, min_amount_out_for_profit},
        optimize::optimize_round_trip,
        prepare::{fetch_account_sequence, prepare_transaction_xdr},
        scanner::ArbOpportunity,
        stats::ArbStats,
        submit::submit_prepared,
    },
    anyhow::{Context, Result},
    router_engine::QuoteHydration,
    tracing::{info, warn},
};

#[derive(Debug, Clone)]
pub struct PreparedArbTx {
    pub route_label: String,
    pub caller_public_key: String,
    pub amount_in: u128,
    pub amount_out: u128,
    pub profit_bps: i64,
    pub unsigned_tx_xdr: String,
    pub simulated: bool,
}

async fn resolve_quote(ctx: &ArbContext, opp: &ArbOpportunity, hydration: &QuoteHydration) -> Option<RoundTripQuote> {
    if ctx.config.optimize_amount {
        optimize_round_trip(
            ctx,
            &opp.quote.base,
            &opp.quote.bridge,
            hydration,
            ctx.config.min_amount_in,
            ctx.config.max_amount_in,
            ctx.config.sample_count,
        )
        .await
    } else {
        quote_round_trip(ctx, &opp.quote.base, &opp.quote.bridge, opp.quote.amount_in, hydration).await
    }
}

pub async fn prepare_opportunity_tx(
    ctx: &ArbContext,
    opp: &ArbOpportunity,
    hydration: &QuoteHydration,
    caller_public_key: &str,
) -> Result<Option<PreparedArbTx>> {
    let Some(aggregator) = ctx.config.aggregator_contract.as_deref() else {
        return Ok(None);
    };

    let Some(quote) = resolve_quote(ctx, opp, hydration).await else {
        return Ok(None);
    };

    let profit = quote.profit();
    let profit_bps = crate::scanner::compute_profit_bps(quote.amount_in, quote.amount_out);
    if profit < ctx.config.min_profit {
        return Ok(None);
    }

    let amount_in_i128 = i128::try_from(quote.amount_in).context("amount_in exceeds i128")?;
    let min_amount_out = min_amount_out_for_profit(quote.amount_in, ctx.config.min_profit);

    let op = build_round_trip_swap_op(
        aggregator,
        caller_public_key,
        &quote.base.canonical(),
        &quote.bridge.canonical(),
        amount_in_i128,
        &quote.leg_out,
        &quote.leg_back,
        min_amount_out,
        &ctx.snapshot,
        hydration,
    )?;

    let seq = fetch_account_sequence(&ctx.config.horizon_url, caller_public_key).await?;
    let fee = 100_000u32;

    let (unsigned_tx_xdr, simulated) = match prepare_transaction_xdr(
        &ctx.config.rpc_url,
        caller_public_key,
        seq as u64,
        std::slice::from_ref(&op),
        fee,
    )
    .await
    {
        Ok(prepared) => (prepared, true),
        Err(e) => {
            warn!(
                error = %e,
                route = %opp.route_label,
                caller = %caller_public_key,
                "Soroban prepare failed; falling back to raw envelope XDR"
            );
            (build_raw_envelope_xdr(caller_public_key, seq as u64, op)?, false)
        }
    };

    info!(
        route = %quote.route_label(),
        caller = %caller_public_key,
        amount_in = quote.amount_in,
        amount_out = quote.amount_out,
        profit_bps,
        simulated,
        min_amount_out,
        leg_out_splits = quote.leg_out.sub_orders.len(),
        leg_back_splits = quote.leg_back.sub_orders.len(),
        "prepared round_trip_swap tx"
    );

    Ok(Some(PreparedArbTx {
        route_label: quote.route_label(),
        caller_public_key: caller_public_key.to_string(),
        amount_in: quote.amount_in,
        amount_out: quote.amount_out,
        profit_bps,
        unsigned_tx_xdr,
        simulated,
    }))
}

pub async fn try_execute_opportunity(
    ctx: &ArbContext,
    opp: &ArbOpportunity,
    hydration: &QuoteHydration,
    caller_pool: &CallerPool,
    stats: &ArbStats,
    dry_run: bool,
) -> Result<()> {
    let Some(guard) = caller_pool.try_acquire().await else {
        warn!(route = %opp.route_label, "all callers busy, dropping opportunity");
        return Ok(());
    };

    let Some(prepared) = prepare_opportunity_tx(ctx, opp, hydration, &guard.public_key()).await? else {
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
        info!(
            route = %prepared.route_label,
            caller = %prepared.caller_public_key,
            "DRY_RUN: would submit round_trip_swap"
        );
        return Ok(());
    }

    submit_prepared(&ctx.config.rpc_url, guard.keypair(), &prepared, stats).await?;
    Ok(())
}

pub fn execution_enabled(config: &ArbConfig) -> bool {
    config.build_tx && config.aggregator_contract.is_some()
}

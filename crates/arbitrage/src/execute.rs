//! Build + optionally submit `aggregator.round_trip_swap` transactions.

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        callers::CallerPool,
        config::ArbConfig,
        context::ArbContext,
        invoke::{build_execute_round_trip_op, build_raw_envelope_xdr, build_round_trip_swap_op},
        optimize::optimize_round_trip,
        prepare::{fetch_account_sequence, prepare_transaction_xdr},
        scanner::ArbOpportunity,
        stats::ArbStats,
        submit::submit_prepared,
        vault::resolve_max_amount_in,
    },
    anyhow::{Context, Result},
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

pub async fn prepare_opportunity_tx(
    ctx: &ArbContext,
    opp: &ArbOpportunity,
    caller_public_key: &str,
    stats: &ArbStats,
) -> Result<Option<PreparedArbTx>> {
    let Some(aggregator) = ctx.config.aggregator_contract.as_deref() else {
        return Ok(None);
    };

    let Some(quote) = resolve_quote(ctx, opp).await else {
        return Ok(None);
    };

    let profit = quote.profit();
    if profit < ctx.config.min_profit {
        return Ok(None);
    }

    let amount_in_i128 = i128::try_from(quote.amount_in).context("amount_in exceeds i128")?;
    let min_amount_out =
        i128::try_from(quote.minimum_out.max(quote.amount_in.saturating_add(1))).context("minimum_out exceeds i128")?;

    let op = if let Some(vault) = ctx.config.vault_contract.as_deref() {
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
        )?
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
        )?
    };

    let seq = fetch_account_sequence(&ctx.config.horizon_url, caller_public_key).await?;
    let fee = 100_000u32;

    let (unsigned_tx_xdr, simulated, simulated_amount_out) = match prepare_transaction_xdr(
        &ctx.config.rpc_url,
        caller_public_key,
        seq as u64,
        std::slice::from_ref(&op),
        fee,
    )
    .await
    {
        Ok(prepared) => (prepared.unsigned_tx_xdr, true, prepared.amount_out),
        Err(err) => {
            warn!(
                error = %err,
                route = %opp.route_label,
                caller = %caller_public_key,
                "Soroban prepare failed"
            );
            (build_raw_envelope_xdr(caller_public_key, seq as u64, op)?, false, 0)
        }
    };

    if simulated {
        let sim_profit = simulated_amount_out.saturating_sub(quote.amount_in);
        if simulated_amount_out < quote.minimum_out || sim_profit < ctx.config.min_profit {
            warn!(
                route = %quote.route_label(),
                caller = %caller_public_key,
                amount_in = quote.amount_in,
                quoted_amount_out = quote.amount_out,
                quoted_minimum_out = quote.minimum_out,
                simulated_amount_out,
                simulated_profit = sim_profit,
                min_profit = ctx.config.min_profit,
                "simulated output below quote-api floor — discard"
            );
            stats
                .txs_sim_profit_rejected
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(None);
        }
    }

    let profit_bps = crate::scanner::compute_profit_bps(quote.amount_in, simulated_amount_out);
    let simulated_profit = simulated_amount_out.saturating_sub(quote.amount_in);

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
        info!(
            route = %prepared.route_label,
            caller = %prepared.caller_public_key,
            simulated_amount_out = prepared.simulated_amount_out,
            simulated_profit = prepared.simulated_amount_out.saturating_sub(prepared.amount_in),
            profit_bps = prepared.profit_bps,
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

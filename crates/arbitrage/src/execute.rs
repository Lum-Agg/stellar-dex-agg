//! Build + optionally submit `aggregator.round_trip_swap` transactions.

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        callers::{CallerPool, DEFAULT_CALLER_COOLDOWN_MS},
        config::ArbConfig,
        context::ArbContext,
        invoke::{
            build_execute_round_trip_op, build_raw_envelope_xdr, build_round_trip_swap_op, min_amount_out_break_even,
        },
        prepare::{
            fetch_account_sequence, parse_base_received_from_sim_error, prepare_transaction_xdr,
            vault_allowance_expiration_cached,
        },
        scanner::ArbOpportunity,
        stats::ArbStats,
        vault::resolve_max_amount_in,
    },
    anyhow::{Context, Result},
    std::sync::Arc,
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
    /// Fee used for profit gates / accounting (stroops): simulated
    /// `min_resource_fee` only (declared inclusion bid is ignored here).
    pub estimated_fee_stroops: u128,
    pub profit_bps: i64,
    pub unsigned_tx_xdr: String,
    pub simulated: bool,
}

/// Reuse scanner quote (already sized/optimized). Cap by config max.
fn quote_for_execute<'a>(ctx: &ArbContext, opp: &'a ArbOpportunity) -> Option<&'a RoundTripQuote> {
    let max_in = resolve_max_amount_in(ctx, &opp.quote.base.canonical());
    if opp.quote.amount_in < ctx.config.min_amount_in || opp.quote.amount_in > max_in {
        return None;
    }
    if opp.quote.profit() < ctx.config.min_profit {
        return None;
    }
    Some(&opp.quote)
}

fn build_op_for_quote(
    ctx: &ArbContext,
    aggregator: &str,
    caller_public_key: &str,
    quote: &RoundTripQuote,
    min_amount_out: i128,
    allowance_expiration_ledger: u32,
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
            allowance_expiration_ledger,
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
        )
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

    let Some(initial) = quote_for_execute(ctx, opp) else {
        return Ok(None);
    };
    let mut quote = initial.clone();

    let seq = fetch_account_sequence(&ctx.config.rpc_url, caller_public_key).await?;
    // Vault reclaim approve expiry: fixed op arg (not vault-side sequence()+N).
    // Cached getLatestLedger — cushion is ~100k ledgers, so a 60s-stale value is
    // fine.
    let allowance_expiration = if ctx.config.vault_contract.is_some() {
        vault_allowance_expiration_cached(&ctx.latest_ledger, &ctx.config.rpc_url).await?
    } else {
        0
    };
    // Declared inclusion bid (priority under surge). Not what we charge in the
    // profit gate — actual inclusion is ~100 stroops and dominated by resource fee.
    let fee = 100_000u32;
    let mut unsigned_tx_xdr = String::new();
    let mut simulated = false;
    let mut simulated_amount_out = 0u128;
    let mut estimated_fee_stroops = 0u128;
    let mut tried_probe_fallback = false;

    // Up to 2 sims: sized quote → optional probe-size fallback on phantom profit.
    for _attempt in 0..2 {
        let min_amount_out = min_amount_out_break_even(quote.amount_in);
        let op = build_op_for_quote(
            ctx,
            aggregator,
            caller_public_key,
            &quote,
            min_amount_out,
            allowance_expiration,
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
                // Profit gate: resource fee only (ignore ~100 stroop inclusion).
                estimated_fee_stroops = prepared.resource_fee_stroops;
                break;
            }
            Err(err) => {
                let err_str = err.to_string();
                let recovered_base_out = parse_base_received_from_sim_error(
                    &err_str,
                    &quote.base.canonical(),
                    aggregator,
                    caller_public_key,
                );

                // Legs ran but break-even assert failed → quote-api phantom profit.
                // Fall back to probe size so small real opportunities are not skipped.
                if !tried_probe_fallback && quote.amount_in > ctx.config.min_amount_in {
                    if let Some(base_out) = recovered_base_out {
                        let on_chain_profit = base_out.saturating_sub(quote.amount_in);
                        let gap_bps = crate::stats::quote_sim_gap_bps(quote.amount_in, quote.amount_out, base_out);
                        stats
                            .discard_size_unprofitable
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        stats.record_quote_sim_gap(quote.amount_in, quote.amount_out, base_out);
                        warn!(
                            route = %quote.route_label(),
                            amount_in = quote.amount_in,
                            quoted_amount_out = quote.amount_out,
                            on_chain_base_out = base_out,
                            on_chain_profit,
                            gap_bps,
                            "optimized size unprofitable on-chain — retry at probe size"
                        );
                        tried_probe_fallback = true;
                        match quote_round_trip(ctx, &quote.base, &quote.bridge, ctx.config.min_amount_in).await {
                            Ok(probe) if probe.profit() >= ctx.config.min_profit => {
                                quote = probe;
                                continue;
                            }
                            Ok(_) => {
                                stats
                                    .txs_sim_profit_rejected
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                stats
                                    .discard_probe_unprofitable
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                return Ok(None);
                            }
                            Err(e) => {
                                warn!(error = %e, "probe-size re-quote failed");
                            }
                        }
                    }
                }

                // Some HostError/InvalidAction cases are still economic failures:
                // the route executed and returned base token, but the final
                // break-even/min_out assert trapped. Treat those as phantom
                // profit, not structural path breakage.
                if let Some(base_out) = recovered_base_out {
                    let on_chain_profit = base_out.saturating_sub(quote.amount_in);
                    let gap_bps = crate::stats::quote_sim_gap_bps(quote.amount_in, quote.amount_out, base_out);
                    stats
                        .txs_sim_profit_rejected
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    stats
                        .discard_below_quoted
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    stats.record_quote_sim_gap(quote.amount_in, quote.amount_out, base_out);
                    warn!(
                        route = %quote.route_label(),
                        caller = %caller_public_key,
                        amount_in = quote.amount_in,
                        quoted_amount_out = quote.amount_out,
                        on_chain_base_out = base_out,
                        on_chain_profit,
                        gap_bps,
                        tried_probe_fallback,
                        "simulated route below quoted profit — discard"
                    );
                    return Ok(None);
                }

                // Structural host traps (InvalidAction / footprint) only count
                // as dead-path failures when we cannot recover a meaningful
                // base-token output from the event log.
                if is_structural_sim_failure(&err_str) {
                    warn!(
                        error = %err,
                        route = %opp.route_label,
                        caller = %caller_public_key,
                        "structural sim failure — discard"
                    );
                    stats
                        .txs_sim_rejected
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Ok(None);
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
        // Gas covered by estimated_fee; ARB_MIN_PROFIT is a post-fee race/slippage
        // buffer.
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
            stats
                .discard_fee_gate
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
    stats: Arc<ArbStats>,
    profit: Arc<crate::profit::ProfitBook>,
    dry_run: bool,
) -> Result<bool> {
    let Some(guard) = caller_pool.try_acquire().await else {
        warn!(route = %opp.route_label, "all callers busy, dropping opportunity");
        return Ok(false);
    };

    let Some(prepared) = prepare_opportunity_tx(ctx, opp, &guard.public_key(), stats.as_ref()).await? else {
        return Ok(false);
    };

    stats.txs_prepared.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if !prepared.simulated {
        stats
            .txs_sim_rejected
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        warn!(route = %prepared.route_label, "simulation missing — skip submit");
        return Ok(false);
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
        return Ok(false);
    }

    let cooldown_ms = std::env::var("ARB_CALLER_COOLDOWN_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_CALLER_COOLDOWN_MS);

    crate::submit::submit_prepared(
        &ctx.config.rpc_url,
        guard.keypair(),
        &prepared,
        stats.clone(),
        profit,
        ctx.config.poll_tx,
    )
    .await?;

    // Release mutex immediately; in-flight cooldown prevents seq reuse for ~one
    // ledger.
    guard.mark_in_flight(cooldown_ms);

    Ok(true)
}

/// Host traps that will not produce a usable assembled tx (retrying the same
/// quote envelope is wasted work).
fn is_structural_sim_failure(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("invalidaction") ||
        e.contains("outside of the footprint") ||
        e.contains("exceededlimit") ||
        (e.contains("footprint") && (e.contains("hosterror") || e.contains("trapped")))
}

pub fn execution_enabled(config: &ArbConfig) -> bool {
    config.build_tx && config.aggregator_contract.is_some()
}

#[cfg(test)]
mod tests {
    use {super::is_structural_sim_failure, crate::prepare::parse_base_received_from_sim_error};

    #[test]
    fn detects_invalid_action() {
        assert!(is_structural_sim_failure(
            "simulation missing i128 return value: HostError: Error(WasmVm, InvalidAction)"
        ));
    }

    #[test]
    fn detects_footprint() {
        assert!(is_structural_sim_failure(
            "trying to access contract data key outside of the footprint"
        ));
    }

    #[test]
    fn ignores_economic_failures() {
        assert!(!is_structural_sim_failure(
            "Error(Contract, #1) assert min_amount_out not met"
        ));
    }

    #[test]
    fn production_invalid_action_can_still_be_economic() {
        let log = r#"
   2: [Failed Diagnostic Event (not emitted)] contract:CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, topics:[error, Error(WasmVm, InvalidAction)], data:["VM call trapped: UnreachableCodeReached", round_trip_swap]
   10: [Failed Contract Event (not emitted)] contract:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA, topics:[transfer, CBDRLPPJBF2LQNGEEGRBHNLFL5QCMKLKP5BUCQVX3HKRTXODHTXEQRZ6, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "native"], data:99990273
   59: [Failed Contract Event (not emitted)] contract:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA, topics:[transfer, GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "native"], data:100000000
"#;
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let base = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
        let caller = "GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN";
        assert!(is_structural_sim_failure(log));
        assert_eq!(
            parse_base_received_from_sim_error(log, base, agg, caller),
            Some(99_990_273)
        );
    }
}

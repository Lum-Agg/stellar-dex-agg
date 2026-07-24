//! One-shot: quote-api round-trip + simulate vault round-trip.

use {
    anyhow::{Context, Result},
    arbitrage::{
        bridge::quote_round_trip,
        config::ArbConfig,
        context::ArbContext,
        invoke::{build_execute_round_trip_op, min_amount_out_break_even},
        prepare::{parse_base_received_from_sim_error, prepare_transaction_xdr},
        scanner::compute_profit_bps,
    },
    router_engine::TokenId,
    std::env,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let bridge_sym = env::args().nth(1).unwrap_or_else(|| "USDC".into());
    let amount_in: u128 = env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(100_000_000);
    let bridge = match bridge_sym.as_str() {
        "USDC" => "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "EURC" => "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
        "AQUA" => "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK",
        "yXLM" => "CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC",
        "SHX" => "CCKCKCPHYVXQD4NECBFJTFSCU2AMSJGCNG4O6K4JVRE2BLPR7WNDBQIQ",
        other if other.starts_with('C') && other.len() == 56 => other,
        other => anyhow::bail!("unknown bridge {other}"),
    };

    let config = ArbConfig::from_env()?;
    let ctx = ArbContext::connect(config.clone()).await?;
    let base = ctx.config.base_tokens.first().context("no base token")?.clone();
    let bridge_id = TokenId::from_str_auto(bridge);

    println!(
        "max_splits={} max_hops={} amount_in={}",
        ctx.config.max_splits, ctx.config.max_hops, amount_in
    );

    let quote = if env::var("ARB_DIAG_FLAT_BACK").ok().as_deref() == Some("1") {
        // User-shaped 2+2: one out quote + one back quote on total bridge
        // (not per-split nested backs that arb uses in production).
        let leg_out = arbitrage::bridge::quote_leg(&ctx, &base, &bridge_id, amount_in).await?;
        let leg_back = arbitrage::bridge::quote_leg(&ctx, &bridge_id, &base, leg_out.route.total_expected_out).await?;
        let amount_out = leg_back.route.total_expected_out;
        let minimum_out = leg_back.minimum_out;
        arbitrage::bridge::RoundTripQuote {
            base: base.clone(),
            bridge: bridge_id.clone(),
            amount_in,
            amount_out,
            minimum_out,
            leg_out,
            leg_back,
        }
    } else {
        quote_round_trip(&ctx, &base, &bridge_id, amount_in)
            .await
            .with_context(|| format!("no round-trip quote for {bridge_sym}"))?
    };

    println!("=== quote-api {bridge_sym} ===");
    println!("quote_api={:?}", ctx.config.quote_api_urls);
    println!("amount_in={}", quote.amount_in);
    println!(
        "amount_out={} minimum_out={} profit={}",
        quote.amount_out,
        quote.minimum_out,
        quote.profit()
    );
    for (leg, name) in [(&quote.leg_out, "leg_out"), (&quote.leg_back, "leg_back")] {
        println!("--- {name} splits={} ---", leg.route.sub_orders.len());
        for sub in &leg.route.sub_orders {
            for (i, hop) in sub.path.sources.iter().enumerate() {
                println!(
                    "  {} {} pool={} {} -> {}",
                    hop,
                    sub.path.pool_addresses[i],
                    sub.path.tokens[i].canonical(),
                    sub.path.tokens[i + 1].canonical(),
                    sub.amount_in
                );
            }
        }
    }

    let vault = ctx.config.vault_contract.as_deref().context("ARB_VAULT_CONTRACT")?;
    let agg = ctx.config.aggregator_contract.as_deref().context("ARB_AGGREGATOR")?;
    let caller = "GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN";
    // Always break-even floor (builder rejects min_out < amount_in).
    // Unprofitable quotes still trap; prepare embeds resource_fee in the error.
    let min_out = min_amount_out_break_even(amount_in);
    let latest = arbitrage::prepare::fetch_latest_ledger(&ctx.config.rpc_url).await?;
    let allowance_exp = arbitrage::prepare::vault_allowance_expiration(latest);
    let op = build_execute_round_trip_op(
        vault,
        agg,
        caller,
        &base.canonical(),
        &bridge_id.canonical(),
        amount_in as i128,
        &quote.leg_out,
        &quote.leg_back,
        min_out,
        allowance_exp,
    )?;

    let seq: u64 = arbitrage::prepare::fetch_account_sequence(&ctx.config.rpc_url, caller).await? as u64;

    match prepare_transaction_xdr(&ctx.config.rpc_url, caller, seq, std::slice::from_ref(&op), 100_000).await {
        Ok(prepared) => {
            println!("=== simulate OK ===");
            println!("amount_out={}", prepared.amount_out);
            println!("profit={}", prepared.amount_out.saturating_sub(amount_in));
            println!("profit_bps={}", compute_profit_bps(amount_in, prepared.amount_out));
            println!(
                "quote_sim_gap_bps={}",
                compute_profit_bps(amount_in, quote.amount_out)
                    .saturating_sub(compute_profit_bps(amount_in, prepared.amount_out))
            );
            println!(
                "resource_fee_stroops={} (~{:.4} XLM)",
                prepared.resource_fee_stroops,
                prepared.resource_fee_stroops as f64 / 1e7
            );
            println!(
                "estimated_fee_stroops={} (inclusion+resource)",
                prepared.estimated_fee_stroops
            );
            println!(
                "leg_out_splits={} leg_back_splits={}",
                quote.leg_out.route.sub_orders.len(),
                quote.leg_back.route.sub_orders.len()
            );
        }
        Err(e) => {
            let err_str = e.to_string();
            let resource_fee = err_str
                .split("resource_fee_stroops=")
                .nth(1)
                .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|s| s.parse::<u128>().ok());
            if let Some(fee) = resource_fee {
                println!(
                    "resource_fee_stroops={} (~{:.4} XLM) [from trap/error path]",
                    fee,
                    fee as f64 / 1e7
                );
            }
            if let Some(recovered) = parse_base_received_from_sim_error(&err_str, &base.canonical(), agg, caller) {
                println!("=== simulate recovered from trap ===");
                println!("amount_out={recovered}");
                println!("profit={}", recovered.saturating_sub(amount_in));
                println!("profit_bps={}", compute_profit_bps(amount_in, recovered));
                println!(
                    "quote_sim_gap_bps={}",
                    compute_profit_bps(amount_in, quote.amount_out)
                        .saturating_sub(compute_profit_bps(amount_in, recovered))
                );
                println!(
                    "leg_out_splits={} leg_back_splits={}",
                    quote.leg_out.route.sub_orders.len(),
                    quote.leg_back.route.sub_orders.len()
                );
            } else {
                println!("=== simulate FAILED ===");
                println!("{e:#}");
            }
        }
    }
    Ok(())
}

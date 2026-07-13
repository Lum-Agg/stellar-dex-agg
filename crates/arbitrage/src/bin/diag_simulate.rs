//! One-shot: quote + simulate vault round-trip; print RPC error detail.
//! Usage on server: source deploy/arb.env; RPC_URL=... SNAPSHOT_REDIS_URL=... \
//!   ARB_VAULT_CONTRACT=... ARB_AGGREGATOR_CONTRACT=... ARB_BRIDGE_TOKENS=... \
//!   cargo run --release -p arbitrage --bin diag_simulate -- USDC

use {
    anyhow::{Context, Result},
    arbitrage::{
        bridge::quote_round_trip,
        collect_paths_for_base,
        config::ArbConfig,
        context::ArbContext,
        hydrate::hydrate_paths,
        invoke::{build_execute_round_trip_op, min_amount_out_break_even},
        prepare::{parse_bridge_received_from_sim_error, prepare_transaction_xdr},
        scanner::compute_profit_bps,
        vault::resolve_max_amount_in,
    },
    router_engine::TokenId,
    std::env,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let bridge_sym = env::args().nth(1).unwrap_or_else(|| "USDC".into());
    let bridge = match bridge_sym.as_str() {
        "USDC" => "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "AQUA" => "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
        other if other.starts_with('C') && other.len() == 56 => other,
        other => anyhow::bail!("unknown bridge {other}"),
    };

    let config = ArbConfig::from_env()?;
    let ctx = ArbContext::connect(config.clone()).await?;
    let base = ctx.config.base_tokens.first().context("no base token")?.clone();
    let bridge_id = TokenId::from_str_auto(bridge);

    let paths = collect_paths_for_base(&ctx, &base).await;
    let (hydration, _) = hydrate_paths(&paths, ctx.pool_store.as_ref()).await;
    let amount_in = 100_000_000u128;
    let Some(quote) = quote_round_trip(&ctx, &base, &bridge_id, amount_in, &hydration).await else {
        anyhow::bail!("no round-trip quote for {bridge_sym}");
    };

    println!("=== quote {bridge_sym} ===");
    println!("amount_in={}", quote.amount_in);
    println!("amount_out={} profit={}", quote.amount_out, quote.profit());
    for (leg, name) in [(&quote.leg_out, "leg_out"), (&quote.leg_back, "leg_back")] {
        println!("--- {name} splits={} ---", leg.sub_orders.len());
        for sub in &leg.sub_orders {
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
    let min_out = min_amount_out_break_even(amount_in);
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
        None,
        &ctx.snapshot,
        &hydration,
    )?;

    let seq: u64 = arbitrage::prepare::fetch_account_sequence(&ctx.config.horizon_url, caller).await? as u64;
    let _max = resolve_max_amount_in(&ctx, &base.canonical()).await;

    match prepare_transaction_xdr(&ctx.config.rpc_url, caller, seq, std::slice::from_ref(&op), 100_000).await {
        Ok(prepared) => {
            println!("=== simulate OK ===");
            println!("amount_out={}", prepared.amount_out);
            println!("profit={}", prepared.amount_out.saturating_sub(amount_in));
        }
        Err(e) => {
            let err_text = format!("{e:#}");
            let bridge_override = parse_bridge_received_from_sim_error(&err_text, &bridge_id.canonical(), agg);
            if let Some(bridge_amt) = bridge_override {
                println!("=== retry with bridge amount {bridge_amt} ===");
                let retry_op = build_execute_round_trip_op(
                    vault,
                    agg,
                    caller,
                    &base.canonical(),
                    &bridge_id.canonical(),
                    amount_in as i128,
                    &quote.leg_out,
                    &quote.leg_back,
                    min_out,
                    Some(bridge_amt),
                    &ctx.snapshot,
                    &hydration,
                )?;
                match prepare_transaction_xdr(
                    &ctx.config.rpc_url,
                    caller,
                    seq,
                    std::slice::from_ref(&retry_op),
                    100_000,
                )
                .await
                {
                    Ok(prepared) => {
                        println!("=== simulate OK (after bridge adjust) ===");
                        println!("amount_out={}", prepared.amount_out);
                        println!("profit={}", prepared.amount_out.saturating_sub(amount_in));
                    }
                    Err(e2) => {
                        println!("=== simulate FAILED (after bridge adjust) ===");
                        println!("{e2:#}");
                    }
                }
            } else {
                println!("=== simulate FAILED ===");
                println!("{err_text}");
            }
        }
    }
    Ok(())
}

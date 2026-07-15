//! One-shot: quote-api round-trip + simulate vault round-trip.

use {
    anyhow::{Context, Result},
    arbitrage::{
        bridge::quote_round_trip, config::ArbConfig, context::ArbContext, invoke::build_execute_round_trip_op,
        prepare::prepare_transaction_xdr, scanner::compute_profit_bps,
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

    let amount_in = 100_000_000u128;
    let quote = quote_round_trip(&ctx, &base, &bridge_id, amount_in)
        .await
        .with_context(|| format!("no round-trip quote for {bridge_sym}"))?;

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
    let min_out = i128::try_from(quote.minimum_out.max(quote.amount_in.saturating_add(1)))?;
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
    )?;

    let seq: u64 = arbitrage::prepare::fetch_account_sequence(&ctx.config.rpc_url, caller).await? as u64;

    match prepare_transaction_xdr(&ctx.config.rpc_url, caller, seq, std::slice::from_ref(&op), 100_000).await {
        Ok(prepared) => {
            println!("=== simulate OK ===");
            println!("amount_out={}", prepared.amount_out);
            println!("profit={}", prepared.amount_out.saturating_sub(amount_in));
            println!("profit_bps={}", compute_profit_bps(amount_in, prepared.amount_out));
        }
        Err(e) => {
            println!("=== simulate FAILED ===");
            println!("{e:#}");
        }
    }
    Ok(())
}

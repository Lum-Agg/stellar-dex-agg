//! Compare local quote-api hop outputs vs on-chain `estimate_swap` / Soroswap
//! reserves.
//!
//! Usage:
//!   QUOTE_API_URL=http://127.0.0.1:3100 RPC_URL=http://127.0.0.1:8003 \
//!     cargo run -p dex-adapters --bin quote-hop-compare -- \
//!     CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA \
//!     CCKCKCPHYVXQD4NECBFJTFSCU2AMSJGCNG4O6K4JVRE2BLPR7WNDBQIQ \
//!     100000000

use {
    anyhow::{Context, Result},
    dex_adapters::{on_chain_quote, rpc::SorobanRpc},
    serde_json::Value,
    std::env,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut args = env::args().skip(1);
    let token_in = args.next().context("token_in")?;
    let token_out = args.next().context("token_out")?;
    let amount_in: u128 = args
        .next()
        .unwrap_or_else(|| "100000000".into())
        .parse()
        .context("amount_in")?;

    let quote_api = env::var("QUOTE_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3100".into());
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".into());
    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");

    let url = format!(
        "{quote_api}/api/v1/quote?token_in={token_in}&token_out={token_out}&amount_in={amount_in}&prefer_soroban=1"
    );
    let body: Value = reqwest::get(&url).await?.json().await?;
    if body["success"].as_bool() != Some(true) {
        anyhow::bail!("quote failed: {}", body);
    }
    let data = &body["data"];
    let expected: u128 = data["expected_output"].as_str().unwrap_or("0").parse()?;
    println!("quote-api expected_output={expected}");
    println!("is_split={}", data["is_split"]);

    let mut chain_total = 0u128;
    for (si, sub) in data["sub_routes"].as_array().context("sub_routes")?.iter().enumerate() {
        let sources: Vec<String> = sub["dex_types"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let pools: Vec<String> = sub["pool_addresses"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let tokens: Vec<String> = sub["path"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let in_indices: Vec<u32> = sub["in_indices"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect();
        let out_indices: Vec<u32> = sub["out_indices"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect();
        let sub_in: u128 = sub["amount_in"].as_str().unwrap_or("0").parse()?;
        let local_out: u128 = sub["amount_out"].as_str().unwrap_or("0").parse()?;

        println!("\n--- sub[{si}] local_in={sub_in} local_out={local_out} sources={sources:?} ---");
        let mut current = sub_in;
        for i in 0..sources.len() {
            let hop_out = on_chain_quote::hop_amount_out_on_chain(
                &rpc,
                &sources[i],
                &pools[i],
                &tokens[i],
                &tokens[i + 1],
                in_indices[i],
                out_indices[i],
                current,
            )
            .await?;
            match hop_out {
                Some(v) => {
                    println!(
                        "  hop[{i}] {} {} in={current} chain_out={v}",
                        sources[i],
                        &pools[i][..12.min(pools[i].len())]
                    );
                    current = v;
                }
                None => {
                    println!("  hop[{i}] {} FAILED", sources[i]);
                    current = 0;
                    break;
                }
            }
        }
        println!(
            "  chain path out={current} delta_vs_local={}",
            local_out as i128 - current as i128
        );
        chain_total = chain_total.saturating_add(current);
    }

    println!("\n=== totals ===");
    println!("local={expected}");
    println!("chain={chain_total}");
    println!("delta={}", expected as i128 - chain_total as i128);
    Ok(())
}

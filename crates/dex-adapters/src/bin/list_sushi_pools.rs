//! List Sushi V3 pool addresses (factory + known list) for syncing
//! KNOWN_POOL_ADDRS.
//!
//!   RPC_URL=... cargo run -p dex-adapters --bin list-sushi-pools

use {
    dex_adapters::{rpc::SorobanRpc, SushiAdapter},
    std::sync::Arc,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let rpc_url =
        std::env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());
    let rpc = Arc::new(SorobanRpc::new(
        &rpc_url,
        "Public Global Stellar Network ; September 2015",
    ));
    let sushi = SushiAdapter::new(rpc);

    let pools = sushi.discover_all_pools().await?;
    let mut pools = pools;
    pools.sort_by(|a, b| a.pool_address.cmp(&b.pool_address));

    println!(
        "// {} pools with liquidity (synced from Sushi factory / explore)",
        pools.len()
    );
    println!("const KNOWN_POOL_ADDRS: &[&str] = &[");
    for p in &pools {
        let fee_pct = p.fee_bps as f64 / 10_000.0;
        println!(
            "    \"{}\", // {} / {} {:.2}%",
            p.pool_address,
            short_token(&p.token_a),
            short_token(&p.token_b),
            fee_pct
        );
    }
    println!("];");
    Ok(())
}

fn short_token(t: &dex_adapters::traits::TokenId) -> String {
    match t {
        dex_adapters::traits::TokenId::Native => "XLM".into(),
        dex_adapters::traits::TokenId::Classic { code, .. } => code.clone(),
        dex_adapters::traits::TokenId::Contract { address } => {
            if address.len() > 12 {
                format!("{}…{}", &address[..6], &address[address.len() - 4..])
            } else {
                address.clone()
            }
        }
    }
}

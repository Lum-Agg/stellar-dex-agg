//! Compare Sushi local CLMM quotes vs on-chain pool `swap` simulation.
//!
//! Usage:
//!   cargo run -p dex-adapters --bin sushi-quote-compare
//!   SUSHI_POOL=CCR2CH4GQVCZ... cargo run -p dex-adapters --bin
//! sushi-quote-compare

use {
    anyhow::Result,
    dex_adapters::{DexAdapter, SorobanRpc, SushiAdapter, TokenId},
    std::sync::Arc,
};

const XLM: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const POOL_XLM_USDC: &str = "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ";

fn diff_bps(local: u128, onchain: u128) -> f64 {
    if onchain == 0 {
        return 100.0;
    }
    ((local as f64 - onchain as f64).abs() / onchain as f64) * 10_000.0
}

fn token_label(addr: &str) -> &str {
    if addr == XLM {
        "XLM"
    } else if addr == USDC {
        "USDC"
    } else {
        &addr[..8]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let rpc_url =
        std::env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());
    println!("RPC: {}\n", rpc_url);

    let rpc = Arc::new(SorobanRpc::new(
        &rpc_url,
        "Public Global Stellar Network ; September 2015",
    ));
    let sushi = SushiAdapter::new(rpc);

    let load_all = std::env::var("SUSHI_LOAD_ALL").ok().as_deref() == Some("1");
    let primary_pool = std::env::var("SUSHI_POOL").unwrap_or_else(|_| POOL_XLM_USDC.to_string());

    if load_all {
        println!("Loading all Sushi pools (slot0 + liquidity + ticks via pool-lens)...");
        sushi.get_trading_pairs().await?;
    } else {
        println!(
            "Loading pool {} (set SUSHI_LOAD_ALL=1 for every pool)...",
            &primary_pool[..16]
        );
        sushi.ensure_pool_loaded(&primary_pool).await?;
    }

    let pool_count = sushi.pool_addresses().await.len();
    println!("{} pool(s) in cache\n", pool_count);

    let mut pools_to_test = vec![primary_pool.clone()];
    if load_all {
        pools_to_test.clear();
        pools_to_test.push(POOL_XLM_USDC.to_string());
        for addr in sushi.pool_addresses().await {
            if pools_to_test.len() >= 3 {
                break;
            }
            if !pools_to_test.contains(&addr) {
                pools_to_test.push(addr);
            }
        }
    }

    let amounts: &[(u128, &str)] = &[(1_000_000, "0.1 XLM"), (10_000_000, "1 XLM"), (100_000_000, "10 XLM")];

    println!("get_quote(): local CLMM only (no router RPC simulate)\n");
    println!(
        "{:<12} {:<10} {:>14} {:>14} {:>9} {:>8}",
        "pool", "amount", "local", "pool_swap", "diff_bps", "ok<0.5%"
    );
    println!("{}", "-".repeat(78));

    for pool in &pools_to_test {
        let Some((t0, t1, fee_ppm, liq)) = sushi.pool_info(pool).await else {
            continue;
        };
        let short = &pool[..12];

        let (token_in, token_out) = if t0 == XLM {
            (XLM, t1.as_str())
        } else if t1 == XLM {
            (XLM, t0.as_str())
        } else {
            (t0.as_str(), t1.as_str())
        };

        if token_in != XLM {
            println!("{:<12} skip (no XLM leg)", short);
            continue;
        }

        println!(
            "{:<12} {}/{} fee={}ppm liq={}",
            short,
            token_label(t0.as_str()),
            token_label(t1.as_str()),
            fee_ppm,
            liq
        );

        for &(amt, label) in amounts {
            let (local, chain, _, sim_err) = sushi.compare_local_vs_simulate(pool, token_in, token_out, amt).await?;

            match (local, chain) {
                (Some(l), Some(s)) => {
                    let d = diff_bps(l, s);
                    println!(
                        "{:<12} {:<10} {:>14} {:>14} {:>9.1} {:>8}",
                        "",
                        label,
                        l,
                        s,
                        d,
                        if d < 50.0 { "YES" } else { "NO" }
                    );
                }
                (Some(l), None) => {
                    let err = sim_err.as_deref().unwrap_or("pool swap returned None");
                    println!(
                        "{:<12} {:<10} {:>14} {:>14} {:>9} {:>8}",
                        "", label, l, "-", "-", "NO_CHAIN"
                    );
                    if label == "1 XLM" {
                        println!("    chain error: {}", err);
                    }
                }
                (None, Some(s)) => println!(
                    "{:<12} {:<10} {:>14} {:>14} {:>9} {:>8}",
                    "", label, "-", s, "-", "NO_LOCAL"
                ),
                _ => println!(
                    "{:<12} {:<10} {:>14} {:>14} {:>9} {:>8}",
                    "", label, "-", "-", "-", "FAIL"
                ),
            }
        }
        println!();
    }

    println!("--- adapter get_quote (1 XLM, XLM/USDC pool) ---");
    let q = sushi
        .get_quote(
            &TokenId::Contract {
                address: XLM.to_string(),
            },
            &TokenId::Contract {
                address: USDC.to_string(),
            },
            10_000_000,
            POOL_XLM_USDC,
        )
        .await?;
    println!("{:?}", q);

    Ok(())
}

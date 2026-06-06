//! On-chain inspection of specific Aquarius pools (run with --ignored
//! --nocapture).

use {
    dex_adapters::rpc::{scval_to_address, scval_to_u128, SorobanRpc},
    std::sync::Arc,
};

const POOLS: &[&str] = &[
    "CBRXOYKXPQI4EEA6KA35TUIYN5OJLNWMTIVDOMNOIL2BG5Y5LEDHUU7V",
    "CDYLKM3DGH5A6QA6QOIITPKG7C4DTZMS2HF75XURORACBHCR6AOE3K33",
];

#[tokio::test]
#[ignore]
async fn inspect_aquarius_pools_onchain() {
    let rpc = Arc::new(SorobanRpc::new(
        "https://soroban-rpc.mainnet.stellar.gateway.fm",
        "Public Global Stellar Network ; September 2015",
    ));
    for p in POOLS {
        println!("\n=== {p} ===");
        let pt = rpc.call_no_args(p, "pool_type").await.unwrap();
        println!("  pool_type: {pt:?}");
        let tokens = rpc.call_no_args(p, "get_tokens").await.unwrap();
        if let stellar_xdr::curr::ScVal::Vec(Some(vec)) = &tokens {
            for (i, item) in vec.0.iter().enumerate() {
                if let Ok(a) = scval_to_address(item) {
                    println!("  token[{i}] = {a}");
                }
            }
        } else {
            println!("  get_tokens: {tokens:?}");
        }
        let reserves = rpc.call_no_args(p, "get_reserves").await.unwrap();
        if let stellar_xdr::curr::ScVal::Vec(Some(vec)) = &reserves {
            for (i, item) in vec.0.iter().enumerate() {
                if let Ok(r) = scval_to_u128(item) {
                    println!("  reserve[{i}] = {r}");
                }
            }
        }
        if let Ok(amp) = rpc.call_no_args(p, "a").await.and_then(|v| scval_to_u128(&v)) {
            println!("  amp = {amp}");
        }
    }
}

const THREE_TOKEN_POOL: &str = "CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV";

#[tokio::test]
#[ignore]
async fn inspect_3token_aquarius_pool() {
    let rpc = Arc::new(SorobanRpc::new(
        "https://soroban-rpc.mainnet.stellar.gateway.fm",
        "Public Global Stellar Network ; September 2015",
    ));
    let p = THREE_TOKEN_POOL;
    println!("\n=== 3-token pool {p} ===");
    let pt = rpc.call_no_args(p, "pool_type").await.unwrap();
    println!("  pool_type: {pt:?}");
    let tokens = rpc.call_no_args(p, "get_tokens").await.unwrap();
    if let stellar_xdr::curr::ScVal::Vec(Some(vec)) = &tokens {
        for (i, item) in vec.0.iter().enumerate() {
            if let Ok(a) = scval_to_address(item) {
                println!("  token[{i}] = {a}");
            }
        }
    }
}

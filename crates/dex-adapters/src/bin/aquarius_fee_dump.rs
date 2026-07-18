//! Dump Aquarius pool `get_fee_fraction` ScVal shapes.
//!
//!   RPC_URL=http://127.0.0.1:8003 cargo run -p dex-adapters --bin aquarius-fee-dump
//!   POOLS=CCMHVBZGY65EIFQZLZFRWMPMM23MWK4P5RFKDFWEPA5NQHENBNWMZETZ,CAESLMGW...

use {
    dex_adapters::rpc::{scval_to_i128, scval_to_u32, SorobanRpc},
    std::env,
    stellar_xdr::curr as xdr,
};

const DEFAULT_POOLS: &[&str] = &[
    // Known 100 bps classic pool (was misparsed as 30 → ~70 bps quote optimism)
    "CCMHVBZGY65EIFQZLZFRWMPMM23MWK4P5RFKDFWEPA5NQHENBNWMZETZ",
];

fn describe(val: &xdr::ScVal) -> String {
    match val {
        xdr::ScVal::U32(v) => format!("U32({v})"),
        xdr::ScVal::I32(v) => format!("I32({v})"),
        xdr::ScVal::U64(v) => format!("U64({v})"),
        xdr::ScVal::I64(v) => format!("I64({v})"),
        xdr::ScVal::I128(p) => format!("I128(hi={},lo={})", p.hi, p.lo),
        xdr::ScVal::U128(p) => format!("U128(hi={},lo={})", p.hi, p.lo),
        other => format!("{other:?}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");

    let pools: Vec<String> = env::var("POOLS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_POOLS.iter().map(|s| (*s).to_string()).collect());

    for pool in pools {
        println!("\n=== pool {pool} ===");
        match rpc.call_no_args(&pool, "get_fee_fraction").await {
            Ok(val) => {
                println!("  get_fee_fraction: {}", describe(&val));
                if let Ok(u) = scval_to_u32(&val) {
                    println!("    as_u32={u}");
                }
                if let Ok(i) = scval_to_i128(&val) {
                    println!("    as_i128={i}");
                }
                // Mirror adapter parse (U32 primary).
                let parsed = match &val {
                    xdr::ScVal::U32(v) => Some(*v),
                    xdr::ScVal::I32(v) if *v >= 0 => Some(*v as u32),
                    _ => scval_to_i128(&val).ok().and_then(|v| u32::try_from(v).ok()),
                };
                println!("    parsed_bps={parsed:?}");
            }
            Err(e) => println!("  error: {e}"),
        }
    }
    Ok(())
}

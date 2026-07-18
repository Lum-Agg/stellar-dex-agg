//! Dump Phoenix factory fee fields for mainnet pools.
//!
//!   RPC_URL=http://127.0.0.1:8003 cargo run -p dex-adapters --bin phoenix-fee-dump

use {
    dex_adapters::rpc::{get_map_field, scval_to_address, scval_to_i128, scval_to_u32, SorobanRpc},
    std::env,
    stellar_xdr::curr as xdr,
};

const FACTORY: &str = "CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI";
const USDC_POOL: &str = "CBENABXP6C4C7WG6KB7JQOTDS5GIIXF3IX3PIYNZFCDZDWUHITO2HZ4S";

fn describe(val: &xdr::ScVal) -> String {
    match val {
        xdr::ScVal::U32(v) => format!("U32({v})"),
        xdr::ScVal::I32(v) => format!("I32({v})"),
        xdr::ScVal::U64(v) => format!("U64({v})"),
        xdr::ScVal::I64(v) => format!("I64({v})"),
        xdr::ScVal::I128(p) => format!("I128(hi={},lo={})", p.hi, p.lo),
        xdr::ScVal::U128(p) => format!("U128(hi={},lo={})", p.hi, p.lo),
        xdr::ScVal::Bool(b) => format!("Bool({b})"),
        xdr::ScVal::Symbol(s) => format!("Symbol({})", String::from_utf8_lossy(s.as_slice())),
        xdr::ScVal::String(s) => format!("String({})", String::from_utf8_lossy(s.as_slice())),
        xdr::ScVal::Map(Some(m)) => format!("Map(len={})", m.0.len()),
        xdr::ScVal::Vec(Some(v)) => format!("Vec(len={})", v.0.len()),
        other => format!("{other:?}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");
    let result = rpc.call_no_args(FACTORY, "query_all_pools_details").await?;
    let entries = match &result {
        xdr::ScVal::Vec(Some(v)) => &v.0,
        _ => anyhow::bail!("unexpected factory return"),
    };

    println!("pools={}", entries.len());
    for entry in entries.iter() {
        let map = match entry {
            xdr::ScVal::Map(Some(m)) => m,
            _ => continue,
        };
        let addr = match get_map_field(map, "pool_address").and_then(|v| scval_to_address(v).ok()) {
            Some(a) => a,
            None => continue,
        };
        if addr != USDC_POOL && env::var("ALL").ok().as_deref() != Some("1") {
            continue;
        }
        println!("\n=== pool {addr} ===");
        for entry in map.0.iter() {
            let key = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8_lossy(s.as_slice()).into_owned(),
                other => format!("{other:?}"),
            };
            println!("  {key}: {}", describe(&entry.val));
            if key.contains("fee") {
                if let Ok(u) = scval_to_u32(&entry.val) {
                    println!("    as_u32={u}");
                }
                if let Ok(i) = scval_to_i128(&entry.val) {
                    println!("    as_i128={i}");
                }
            }
        }
    }
    Ok(())
}

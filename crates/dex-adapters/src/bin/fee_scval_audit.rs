//! Audit on-chain fee ScVal shapes across DEX adapters.
//!
//!   RPC_URL=http://127.0.0.1:8003 \
//!   AQUA_CLMM_POOLS=addr1,addr2 SUSHI_POOLS=addr1 COMET_POOLS=addr1 \
//!   cargo run -p dex-adapters --bin fee-scval-audit --release

use {
    dex_adapters::rpc::{get_map_field, scval_to_address, scval_to_i128, scval_to_u32, SorobanRpc},
    std::env,
    stellar_xdr::curr as xdr,
};

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

fn parse_multi(val: &xdr::ScVal) -> Option<u32> {
    match val {
        xdr::ScVal::U32(v) => Some(*v),
        xdr::ScVal::I32(v) if *v >= 0 => Some(*v as u32),
        xdr::ScVal::U64(v) if *v <= u32::MAX as u64 => Some(*v as u32),
        xdr::ScVal::I64(v) if *v >= 0 && *v <= u32::MAX as i64 => Some(*v as u32),
        _ => scval_to_i128(val).ok().and_then(|v| u32::try_from(v).ok()),
    }
}

fn parse_u32_only(val: &xdr::ScVal) -> Option<u32> {
    match val {
        xdr::ScVal::U32(v) => Some(*v),
        _ => None,
    }
}

fn parse_comet(val: &xdr::ScVal) -> Option<i128> {
    match val {
        xdr::ScVal::I128(parts) => Some(((parts.hi as i128) << 64) | (parts.lo as u64 as i128)),
        xdr::ScVal::U128(parts) => Some(((parts.hi as u128) << 64 | parts.lo as u128) as i128),
        xdr::ScVal::I64(v) => Some(*v as i128),
        xdr::ScVal::U64(v) => Some(*v as i128),
        xdr::ScVal::U32(v) => Some(*v as i128),
        xdr::ScVal::I32(v) => Some(*v as i128),
        _ => None,
    }
}

fn env_pools(key: &str) -> Vec<String> {
    env::var(key)
        .ok()
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

async fn dump(rpc: &SorobanRpc, label: &str, contract: &str, method: &str) {
    match rpc.call_no_args(contract, method).await {
        Ok(val) => {
            println!(
                "[{label}] {contract} {method} => {} | multi={:?} u32_only={:?} comet={:?} raw_u32={:?} raw_i128={:?}",
                describe(&val),
                parse_multi(&val),
                parse_u32_only(&val),
                parse_comet(&val),
                scval_to_u32(&val).ok(),
                scval_to_i128(&val).ok()
            );
        }
        Err(e) => println!("[{label}] {contract} {method} ERROR: {e}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");

    println!("=== Phoenix factory total_fee_bps ===");
    let factory = "CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI";
    match rpc.call_no_args(factory, "query_all_pools_details").await {
        Ok(xdr::ScVal::Vec(Some(entries))) => {
            let mut n = 0usize;
            for entry in entries.0.iter() {
                let map = match entry {
                    xdr::ScVal::Map(Some(m)) => m,
                    _ => continue,
                };
                let Some(addr) = get_map_field(map, "pool_address").and_then(|v| scval_to_address(v).ok()) else {
                    continue;
                };
                if let Some(fee) = get_map_field(map, "total_fee_bps") {
                    println!(
                        "[phoenix] {addr} total_fee_bps={} | multi={:?} u32_only={:?}  << u32_only fails on I64 = old bug",
                        describe(fee),
                        parse_multi(fee),
                        parse_u32_only(fee)
                    );
                    n += 1;
                    if n >= 5 {
                        break;
                    }
                }
            }
        }
        Ok(other) => println!("unexpected: {}", describe(&other)),
        Err(e) => println!("error: {e}"),
    }

    println!("\n=== Aquarius classic get_fee_fraction ===");
    for pool in [
        "CCMHVBZGY65EIFQZLZFRWMPMM23MWK4P5RFKDFWEPA5NQHENBNWMZETZ",
        "CBRXOYKXPQI4EEA6KA35TUIYN5OJLNWMTIVDOMNOIL2BG5Y5LEDHUU7V",
        "CCCDPF74BFBIHCBWCA3QX5R2UULH4VSJFOK6KL44KDKJS75ZKJJYUSPH",
    ] {
        dump(&rpc, "aqua", pool, "get_fee_fraction").await;
    }

    println!("\n=== Aquarius CLMM get_fee_fraction ===");
    let clmm = env_pools("AQUA_CLMM_POOLS");
    if clmm.is_empty() {
        println!("(pass AQUA_CLMM_POOLS=...)");
    }
    for pool in &clmm {
        dump(&rpc, "aqua_clmm", pool, "get_fee_fraction").await;
    }

    println!("\n=== Sushi fee() ===");
    let sushi = env_pools("SUSHI_POOLS");
    if sushi.is_empty() {
        println!("(pass SUSHI_POOLS=...)");
    }
    for pool in &sushi {
        dump(&rpc, "sushi", pool, "fee").await;
    }

    println!("\n=== Comet get_swap_fee ===");
    let mut comet = env_pools("COMET_POOLS");
    if comet.is_empty() {
        comet.push("CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM".into());
    }
    for pool in &comet {
        dump(&rpc, "comet", pool, "get_swap_fee").await;
    }

    println!("\n=== Soroswap ===");
    println!("[soroswap] no on-chain fee getter — adapter hardcodes fee_bps=30 (Uniswap V2 style)");

    Ok(())
}

//! 3-token Aquarius stableswap: on-chain discovery smoke test (ignored by default).

use dex_adapters::rpc::{scval_to_address, scval_to_u128, SorobanRpc};
use std::sync::Arc;

const ROUTER: &str = "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK";

/// Scan router token sets and report any pool whose `get_tokens()` returns 3 coins.
#[tokio::test]
#[ignore = "network integration — run with --ignored --nocapture"]
async fn scan_mainnet_for_3token_stableswap_pools() {
    let rpc = Arc::new(SorobanRpc::new(
        "https://soroban-rpc.mainnet.stellar.gateway.fm",
        "Public Global Stellar Network ; September 2015",
    ));

    let count_val = rpc.call_no_args(ROUTER, "get_tokens_sets_count").await.unwrap();
    let total = scval_to_u128(&count_val).unwrap();
    let mut found = 0usize;
    let batch = 50u128;
    let mut start = 0u128;

    while start < total {
        let end = (start + batch).min(total);
        let start_val = stellar_xdr::curr::ScVal::U128(stellar_xdr::curr::UInt128Parts {
            hi: (start >> 64) as u64,
            lo: start as u64,
        });
        let end_val = stellar_xdr::curr::ScVal::U128(stellar_xdr::curr::UInt128Parts {
            hi: (end >> 64) as u64,
            lo: end as u64,
        });
        let result = rpc
            .simulate_call(
                ROUTER,
                "get_pools_for_tokens_range",
                vec![start_val, end_val],
            )
            .await
            .unwrap();

        let mut pools = std::collections::HashSet::new();
        if let stellar_xdr::curr::ScVal::Vec(Some(entries)) = &result {
            for entry in entries.0.iter() {
                if let stellar_xdr::curr::ScVal::Vec(Some(pair)) = entry {
                    if let Some(stellar_xdr::curr::ScVal::Map(Some(map))) = pair.0.get(1) {
                        for map_entry in map.0.iter() {
                            if let Ok(addr) = scval_to_address(&map_entry.val) {
                                pools.insert(addr);
                            }
                        }
                    }
                }
            }
        }

        for pool in pools {
            let pt = rpc.call_no_args(&pool, "pool_type").await.unwrap();
            let pt_name = format!("{pt:?}");
            if !pt_name.contains("stable") {
                continue;
            }
            let tokens = rpc.call_no_args(&pool, "get_tokens").await.unwrap();
            let n = match &tokens {
                stellar_xdr::curr::ScVal::Vec(Some(v)) => v.0.len(),
                _ => 0,
            };
            if n == 3 {
                found += 1;
                println!("3-token stable pool {pool} type={pt_name}");
                if let stellar_xdr::curr::ScVal::Vec(Some(v)) = &tokens {
                    for (i, item) in v.0.iter().enumerate() {
                        if let Ok(a) = scval_to_address(item) {
                            println!("  token[{i}] = {a}");
                        }
                    }
                }
                if let Ok(amp) = rpc
                    .call_no_args(&pool, "a")
                    .await
                    .and_then(|v| scval_to_u128(&v))
                {
                    println!("  amp = {amp}");
                }
            }
        }

        start = end;
    }

    println!("scan complete: {found} three-token stable pools on mainnet");
}

//! CLMM accuracy test: compare local tick-math computation vs on-chain
//! simulate.
//!
//! Tests Aquarius concentrated pools and Sushi V3 pools.
//! Run with: cargo test --test clmm_accuracy_test -- --ignored --nocapture
//!
//! Requires network access to Stellar mainnet RPC.

use {
    dex_adapters::{
        aquarius_clmm::AquariusClmmAdapter,
        clmm_math,
        rpc::{scval_to_address, scval_to_i128, scval_to_u128, SorobanRpc},
        traits::TokenId,
        DexAdapter,
    },
    std::sync::Arc,
    stellar_xdr::curr as xdr,
};

/// XLM SAC (Stellar Asset Contract) address on mainnet
const XLM_CONTRACT: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
/// USDC SAC address on mainnet
const USDC_CONTRACT: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

/// Aquarius Router
const AQUARIUS_ROUTER: &str = "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK";

/// Sushi V3 Pool Lens (for reading tick data)
const SUSHI_POOL_LENS: &str = "CDFGDFKEN7EVMI3DKIEQ6BKDAKEPHTEPWC6G2ZTDY7ATVCLD24AAU2IN";

/// Sushi V3 Factory
const SUSHI_FACTORY: &str = "CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF";

fn mainnet_rpc() -> Arc<SorobanRpc> {
    Arc::new(SorobanRpc::mainnet())
}

/// Helper: simulate a swap on an Aquarius concentrated pool contract.
/// Returns (amount0, amount1) where positive = user pays, negative = user
/// receives.
async fn aquarius_clmm_simulate_swap(
    rpc: &SorobanRpc,
    pool_address: &str,
    zero_for_one: bool,
    amount_in: i128,
) -> Option<(i128, i128)> {
    // Aquarius concentrated pool has: simulate_swap(zero_for_one: bool,
    // amount_specified: i128, sqrt_price_limit_x96: U256) Returns (amount0:
    // i128, amount1: i128)
    let zero_for_one_val = xdr::ScVal::Bool(zero_for_one);
    let amount_val = xdr::ScVal::I128(xdr::Int128Parts {
        hi: (amount_in >> 64) as i64,
        lo: amount_in as u64,
    });
    // sqrt_price_limit = 0 (no limit)
    let limit_val = xdr::ScVal::U256(xdr::UInt256Parts {
        hi_hi: 0,
        hi_lo: 0,
        lo_hi: 0,
        lo_lo: 0,
    });

    let args = vec![zero_for_one_val, amount_val, limit_val];

    match rpc.simulate_call(pool_address, "simulate_swap", args).await {
        Ok(result) => {
            // Result is a tuple (i128, i128)
            if let xdr::ScVal::Vec(Some(vec)) = &result {
                if vec.0.len() >= 2 {
                    let a0 = scval_to_i128(&vec.0[0]).ok()?;
                    let a1 = scval_to_i128(&vec.0[1]).ok()?;
                    return Some((a0, a1));
                }
            }
            // Try map format
            if let xdr::ScVal::Map(Some(map)) = &result {
                if map.0.len() >= 2 {
                    let a0 = scval_to_i128(&map.0[0].val).ok()?;
                    let a1 = scval_to_i128(&map.0[1].val).ok()?;
                    return Some((a0, a1));
                }
            }
            println!(
                "  Unexpected simulate_swap result format: {:?}",
                std::mem::discriminant(&result)
            );
            None
        }
        Err(e) => {
            println!("  simulate_swap failed: {}", e);
            None
        }
    }
}

/// Helper: simulate a swap on a Sushi V3 pool via pool-lens
/// quote_exact_input_single.
async fn sushi_simulate_quote(
    rpc: &SorobanRpc,
    token_in: &str,
    token_out: &str,
    fee: u32,
    amount_in: i128,
) -> Option<i128> {
    let token_in_hash = stellar_strkey::Contract::from_string(token_in).ok()?.0;
    let token_out_hash = stellar_strkey::Contract::from_string(token_out).ok()?.0;

    // Call pool-lens quote_exact_input_single(token_in, token_out, fee, amount_in,
    // sqrt_price_limit_x96)
    let args = vec![
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
        xdr::ScVal::U32(fee),
        xdr::ScVal::I128(xdr::Int128Parts {
            hi: (amount_in >> 64) as i64,
            lo: amount_in as u64,
        }),
        // sqrt_price_limit = 0 (no limit)
        xdr::ScVal::U256(xdr::UInt256Parts {
            hi_hi: 0,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        }),
    ];

    match rpc
        .simulate_call(SUSHI_POOL_LENS, "quote_exact_input_single", args)
        .await
    {
        Ok(result) => {
            // Result is Result<i128>
            scval_to_i128(&result).ok()
        }
        Err(e) => {
            println!("  Sushi quote failed: {}", e);
            None
        }
    }
}

/// Discover Aquarius concentrated pools containing XLM.
async fn find_aquarius_clmm_pools_with_xlm(rpc: &SorobanRpc) -> Vec<String> {
    let mut pool_addresses = Vec::new();

    // Query in batches to cover more pools
    for batch_start in (0u128..200).step_by(50) {
        let batch_end = batch_start + 50;
        let start_val = xdr::ScVal::U128(xdr::UInt128Parts {
            hi: 0,
            lo: batch_start as u64,
        });
        let end_val = xdr::ScVal::U128(xdr::UInt128Parts {
            hi: 0,
            lo: batch_end as u64,
        });

        match rpc
            .simulate_call(AQUARIUS_ROUTER, "get_pools_for_tokens_range", vec![start_val, end_val])
            .await
        {
            Ok(result) => {
                if let xdr::ScVal::Vec(Some(entries)) = &result {
                    for entry in entries.0.iter() {
                        if let xdr::ScVal::Vec(Some(pair)) = entry {
                            if pair.0.len() >= 2 {
                                let has_xlm = check_tokens_contain_xlm(&pair.0[0]);
                                if has_xlm {
                                    if let xdr::ScVal::Map(Some(map)) = &pair.0[1] {
                                        for map_entry in map.0.iter() {
                                            if let Ok(addr) = scval_to_address(&map_entry.val) {
                                                pool_addresses.push(addr);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if batch_start == 0 {
                    println!("Failed to query Aquarius router: {}", e);
                }
                break; // Likely past the end
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    pool_addresses
}

fn check_tokens_contain_xlm(val: &xdr::ScVal) -> bool {
    if let xdr::ScVal::Vec(Some(vec)) = val {
        for item in vec.0.iter() {
            if let Ok(addr) = scval_to_address(item) {
                if addr == XLM_CONTRACT {
                    return true;
                }
            }
        }
    }
    false
}

/// Test: Read Aquarius concentrated pool state and compare local vs simulate
/// quote.
#[tokio::test]
#[ignore] // requires network
async fn test_aquarius_clmm_quote_accuracy() {
    let rpc = mainnet_rpc();

    println!("=== Aquarius CLMM Quote Accuracy Test ===\n");

    // 1. Find concentrated pools with XLM
    println!("Discovering Aquarius concentrated pools with XLM...");
    let pool_addresses = find_aquarius_clmm_pools_with_xlm(&rpc).await;
    println!("Found {} candidate pools", pool_addresses.len());

    if pool_addresses.is_empty() {
        println!("No pools found via router query.");
        return;
    }

    // 2. For each pool, check if it's concentrated by calling pool_type()
    let mut concentrated_pools = Vec::new();
    for pool_addr in &pool_addresses {
        match rpc.call_no_args(pool_addr, "pool_type").await {
            Ok(result) => {
                if let xdr::ScVal::Symbol(s) = &result {
                    let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                    if name == "concentrated" {
                        println!("  ✅ Concentrated pool: {}", pool_addr);
                        concentrated_pools.push(pool_addr.clone());
                    } else {
                        // Skip non-concentrated pools silently
                    }
                }
            }
            Err(_) => {}
        }
    }

    println!("\nFound {} concentrated pools with XLM", concentrated_pools.len());

    if concentrated_pools.is_empty() {
        println!("No concentrated pools found. The first 50 token sets may not have concentrated XLM pools.");
        println!("Try increasing the range or checking specific known pool addresses.");
        return;
    }

    // 3. For each concentrated pool, compare on-chain estimate_swap vs local
    //    computation
    let adapter = AquariusClmmAdapter::new(rpc.clone());

    for pool_addr in concentrated_pools.iter().take(3) {
        println!("\n--- Pool: {} ---", pool_addr);

        // Read token0 to determine swap direction
        let token0_addr = match rpc.call_no_args(pool_addr, "get_tokens").await {
            Ok(xdr::ScVal::Vec(Some(vec))) if !vec.0.is_empty() => scval_to_address(&vec.0[0]).unwrap_or_default(),
            _ => {
                println!("  Cannot read tokens, skipping");
                continue;
            }
        };

        let zero_for_one = token0_addr == XLM_CONTRACT;
        let (in_idx, out_idx) = if zero_for_one { (0u32, 1u32) } else { (1u32, 0u32) };
        println!(
            "  XLM is token{}, zero_for_one={}",
            if zero_for_one { 0 } else { 1 },
            zero_for_one
        );

        // On-chain estimate_swap(in_idx, out_idx, in_amount)
        let amount_in: u128 = 100_0000000; // 100 XLM
        let in_idx_val = xdr::ScVal::U32(in_idx);
        let out_idx_val = xdr::ScVal::U32(out_idx);
        let amount_val = xdr::ScVal::U128(xdr::UInt128Parts {
            hi: (amount_in >> 64) as u64,
            lo: amount_in as u64,
        });

        let on_chain_out = match rpc
            .simulate_call(pool_addr, "estimate_swap", vec![in_idx_val, out_idx_val, amount_val])
            .await
        {
            Ok(result) => match scval_to_u128(&result) {
                Ok(v) => {
                    println!("  On-chain estimate_swap: 100 XLM -> {} (raw)", v);
                    Some(v)
                }
                Err(_) => {
                    println!(
                        "  estimate_swap returned unexpected format: {:?}",
                        std::mem::discriminant(&result)
                    );
                    None
                }
            },
            Err(e) => {
                println!("  estimate_swap failed: {}", e);
                None
            }
        };

        // Local computation
        if let Some(expected_out) = on_chain_out {
            println!("  Loading pool state for local computation...");

            // Direct debug: try reading instance storage
            let contract_hash = stellar_strkey::Contract::from_string(pool_addr).unwrap().0;
            let instance_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
                contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
                key: xdr::ScVal::LedgerKeyContractInstance,
                durability: xdr::ContractDataDurability::Persistent,
            });

            match rpc.get_ledger_entries(vec![instance_key]).await {
                Ok(entries) => {
                    println!("  getLedgerEntries returned {} entries", entries.len());
                    if let Some(entry) = entries.first() {
                        // Print the entry type
                        println!("  Entry data type: {:?}", std::mem::discriminant(&entry.entry.data));
                        if let xdr::LedgerEntryData::ContractData(data) = &entry.entry.data {
                            println!("  ContractData val type: {:?}", std::mem::discriminant(&data.val));
                            if let xdr::ScVal::ContractInstance(instance) = &data.val {
                                if let Some(storage) = &instance.storage {
                                    println!("  Instance storage has {} entries", storage.0.len());
                                    for (i, item) in storage.0.iter().take(5).enumerate() {
                                        let key_str = format!("{:?}", &item.key).chars().take(80).collect::<String>();
                                        println!("    [{}] key={}", i, key_str);
                                    }
                                } else {
                                    println!("  Instance has no storage map!");
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("  getLedgerEntries failed: {}", e);
                }
            }

            // Direct debug: try the simulate-based approach
            println!("  Trying get_slot0...");
            match rpc.call_no_args(pool_addr, "get_slot0").await {
                Ok(val) => println!("  get_slot0 OK: {:?}", std::mem::discriminant(&val)),
                Err(e) => println!("  get_slot0 FAILED: {}", e),
            }
            println!("  Trying get_active_liquidity...");
            match rpc.call_no_args(pool_addr, "get_active_liquidity").await {
                Ok(val) => println!("  get_active_liquidity OK: {:?}", val),
                Err(e) => println!("  get_active_liquidity FAILED: {}", e),
            }

            adapter.set_pool_addresses(vec![pool_addr.clone()]).await;
            match adapter.get_trading_pairs().await {
                Ok(pairs) => {
                    println!("  Loaded {} pairs from adapter", pairs.len());
                    if let Some(pair) = pairs.first() {
                        let token_in = if zero_for_one { &pair.token_a } else { &pair.token_b };
                        let token_out = if zero_for_one { &pair.token_b } else { &pair.token_a };

                        match adapter.get_quote(token_in, token_out, amount_in, pool_addr).await {
                            Ok(Some(quote)) => {
                                let local_out = quote.amount_out;
                                let diff = if local_out > expected_out {
                                    local_out - expected_out
                                } else {
                                    expected_out - local_out
                                };
                                let diff_pct = if expected_out != 0 {
                                    (diff as f64 / expected_out as f64) * 100.0
                                } else {
                                    0.0
                                };

                                println!("  Local compute:    {} (raw)", local_out);
                                println!("  Difference:       {} ({:.6}%)", diff, diff_pct);

                                if diff_pct < 0.01 {
                                    println!("  ✅ MATCH (< 0.01% difference)");
                                } else if diff_pct < 1.0 {
                                    println!("  ⚠️  CLOSE (< 1% difference)");
                                } else {
                                    println!("  ❌ MISMATCH ({:.2}% difference)", diff_pct);
                                }
                            }
                            Ok(None) => {
                                println!("  Local compute: no quote (insufficient tick data?)");
                            }
                            Err(e) => {
                                println!("  Local compute error: {}", e);
                            }
                        }
                    } else {
                        println!("  No pairs returned from adapter");
                    }
                }
                Err(e) => {
                    println!("  ❌ Failed to load pool for local quote: {}", e);
                }
            }
        }
    }
}

/// Test: Sushi V3 local CLMM quote accuracy vs simulate.

#[tokio::test]
#[ignore] // requires network
async fn test_sushi_v3_local_quote_accuracy() {
    let rpc = mainnet_rpc();

    println!("=== Sushi V3 Local Quote Accuracy Test ===");
    println!("=== Testing all XLM pools with multiple amounts ===\n");

    let xlm = XLM_CONTRACT;

    // All 50 Sushi pool addresses from factory storage
    let all_sushi_pools: Vec<&str> = vec![
        "CABMZD6BYKKLHRJNS5MURYOBX77NPAH767AI7EVFGWV3WZV55QFN5YNE",
        "CAFLJXGUAURAMBA3AIHC7ZJOAQKGZ7WEFFGMH5XRC35IMNU7PWIBXVTP",
        "CAKWXQDEVVUF2ABUEM3M2G7QJGJNDZNNVXJZYG4Z4QP6K54QTWV4DW2S",
        "CALM7JTAJC7AJ7ZGTQKXZNNILJUCD2AZNN7QA7FVM3YYIJBCJGUABEDH",
        "CAMUA6N6SLCMSIQRICXIQBRYYG3SCEPILS2HJTBXGMCOBCRWQUHAS2OJ",
        "CAOGXY6DW2KWUVOWCGGPLW7MIJNP7XCMXY736LNLOKYEQA3CBKXVIDEA",
        "CAPT5THGW7WOCX47TICCB5JZZK4Y24CHQIBSM57Y472WFFV6FGTRKJQD",
        "CAPUAZDFH4VBQTC7PYL7UM2KSXER2ZY3D462WW6DCE2HGUSO646S4F2X",
        "CAUBW4ARD42U2UEIA7GDUB5LNKTRTVYJHXKL3CV27YZRDFADDGKLZWFD",
        "CAWN3BM2ADBMA4CQZLIHTBXA3BQHV4VAPK42LWT5ONAKZW6PH2BBCKLS",
        "CAWWOFOEGWPPNP6QKVHTJYB7UHRXC6W6EAFMUPGHMJL7K46E6UCOSNDM",
        "CAXJ2FDV6S3L46EFEFRXUBLQ5U5CZLZOG35RPCJRNQVLM5MH2HCK5I7J",
        "CA5MIPAAG3UULVAHK7U3U6VBBM52YIHMCZOOSHNTPUSLYR7NKNHVD6WK",
        "CA5R5L7QE7WC2M4YAPSBITV7M2R6LX5366UURH3REHQMOJV6R5QWTH2K",
        "CA6LYAEDN7XHOKD5TNRFBM3IDFD22VRVVXTGPGK77FZFC7X2OYUQ7BAZ",
        "CA75VVHLWSM7W6ULNQI7ZJYDFOMQCCPKIDDDHBAL5KOKHWWKWQ5S7MHO",
        "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ",
        "CCRKQ2RHBWB5ZCHOSBSYEC2QNVSU3MGVUF56BWWKJMJIJ3ZF2A6W7KEC",
        "CBVKO35SAF2ZT75FCLCGLYQG3S6B32YZTOJ2G5F7M746UGBRAWZ5BNZ6",
        "CDGIQQBPGXATIEXWTFN5O6J7LM5IMQLMVIQ47Q4H44VIMMOBZ4N6KRVZ",
    ];

    // Find which pools have XLM as token0 and have liquidity
    let mut xlm_pools: Vec<(String, String, u32)> = Vec::new();

    for pool_addr in &all_sushi_pools {
        let t0 = match rpc.call_no_args(pool_addr, "token0").await {
            Ok(v) => scval_to_address(&v).unwrap_or_default(),
            Err(_) => continue,
        };
        if t0 != xlm {
            continue;
        }

        let t1 = match rpc.call_no_args(pool_addr, "token1").await {
            Ok(v) => scval_to_address(&v).unwrap_or_default(),
            Err(_) => continue,
        };
        let fee = match rpc.call_no_args(pool_addr, "fee").await {
            Ok(xdr::ScVal::U32(f)) => f,
            _ => continue,
        };
        let liq = match rpc.call_no_args(pool_addr, "liquidity").await {
            Ok(v) => scval_to_u128(&v).unwrap_or(0),
            Err(_) => 0,
        };
        if liq > 0 {
            xlm_pools.push((pool_addr.to_string(), t1, fee));
            println!("  Found XLM pool: {} fee={} liq={}", pool_addr, fee, liq);
        }
    }

    println!("Found {} XLM pools with liquidity\n", xlm_pools.len());

    // For each XLM pool: read state, read ticks via pool-lens, compute locally
    let test_amounts: Vec<u128> = vec![
        1_0000000,    // 1 XLM
        10_0000000,   // 10 XLM
        100_0000000,  // 100 XLM
        1000_0000000, // 1000 XLM
    ];

    let mut total_tested = 0;
    let mut total_matched = 0;

    for (pool_addr, token1_addr, fee) in &xlm_pools {
        println!("\n--- Pool: {} (fee={} ppm) ---", pool_addr, fee);
        println!("  XLM / {}...", &token1_addr[..16]);

        // Read pool state
        let slot0_val = match rpc.call_no_args(pool_addr, "slot0").await {
            Ok(v) => v,
            Err(_) => {
                println!("  slot0 failed");
                continue;
            }
        };
        let (sqrt_price, tick) = match parse_sushi_slot0(&slot0_val) {
            Some(v) => v,
            None => {
                println!("  cannot parse slot0");
                continue;
            }
        };
        let liquidity = match rpc.call_no_args(pool_addr, "liquidity").await {
            Ok(v) => scval_to_u128(&v).unwrap_or(0),
            Err(_) => 0,
        };
        let tick_spacing = match rpc.call_no_args(pool_addr, "tick_spacing").await {
            Ok(xdr::ScVal::I32(v)) => v,
            _ => 60,
        };

        if liquidity == 0 {
            println!("  no liquidity");
            continue;
        }
        println!("  tick={}, liq={}, spacing={}", tick, liquidity, tick_spacing);

        // Read tick data via pool-lens
        let pool_hash = stellar_strkey::Contract::from_string(pool_addr).unwrap().0;
        let pool_scval = xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash))));

        let compressed_tick = floor_div(tick, tick_spacing);
        let current_word = floor_div(compressed_tick, 256);

        let mut tick_store = dex_adapters::clmm_math::TickDataStore::new();
        let scan_range = 10i32;

        for word_pos in (current_word - scan_range)..=(current_word + scan_range) {
            let args = vec![pool_scval.clone(), xdr::ScVal::I32(word_pos)];
            match rpc
                .simulate_call(
                    dex_adapters::sushi::SUSHI_POOL_LENS,
                    "get_populated_ticks_in_word",
                    args,
                )
                .await
            {
                Ok(xdr::ScVal::Vec(Some(ticks_vec))) => {
                    for tick_val in ticks_vec.0.iter() {
                        if let Some((t, lg, ln)) = parse_populated_tick_for_test(tick_val) {
                            if lg > 0 {
                                let comp = floor_div(t, tick_spacing);
                                let chunk_pos = comp.div_euclid(16);
                                let slot = comp.rem_euclid(16) as usize;
                                let chunk = tick_store.chunks.entry(chunk_pos).or_insert_with(|| {
                                    vec![
                                        dex_adapters::clmm_math::TickState {
                                            liquidity_gross: 0,
                                            liquidity_net: 0
                                        };
                                        16
                                    ]
                                });
                                chunk[slot] = dex_adapters::clmm_math::TickState {
                                    liquidity_gross: lg,
                                    liquidity_net: ln,
                                };
                                // Set bitmap
                                let bm_word = chunk_pos >> 8;
                                let bm_bit = (chunk_pos & 255) as u32;
                                let word = tick_store.chunk_bitmap.entry(bm_word).or_insert([0u8; 32]);
                                let byte_idx = 31 - (bm_bit / 8) as usize;
                                word[byte_idx] |= 1u8 << (bm_bit % 8);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let loaded_ticks: usize = tick_store
            .chunks
            .values()
            .map(|c| c.iter().filter(|t| t.liquidity_gross > 0).count())
            .sum();
        println!("  loaded {} ticks", loaded_ticks);

        if loaded_ticks == 0 {
            println!("  no ticks loaded");
            continue;
        }

        // Get oracle hints for simulate
        let hints_val = match rpc.call_no_args(pool_addr, "get_oracle_hints").await {
            Ok(v) => v,
            Err(_) => {
                println!("  hints failed");
                continue;
            }
        };

        // fee conversion: Sushi ppm -> our bps
        let fee_bps_for_math = fee / 100;

        let pool_state = dex_adapters::clmm_math::ClmmPoolState {
            sqrt_price_x96: sqrt_price,
            tick,
            liquidity,
            fee_bps: fee_bps_for_math,
            tick_spacing,
            token0: xlm.to_string(),
            token1: token1_addr.clone(),
        };

        for &amount_in in &test_amounts {
            let xlm_human = amount_in as f64 / 10_000_000.0;

            // Local computation
            let local_out = match dex_adapters::clmm_math::simulate_swap(&pool_state, &tick_store, amount_in, true) {
                Some((out, _, _)) => out,
                None => {
                    println!("  {:>6.0} XLM: local=0 (no output)", xlm_human);
                    continue;
                }
            };

            // On-chain simulate
            let dummy = xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([0u8; 32])),
            )));
            let price_limit = xdr::ScVal::U256(xdr::UInt256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 4295128740,
            });
            let swap_args = vec![
                dummy.clone(),
                dummy.clone(),
                xdr::ScVal::Bool(true),
                xdr::ScVal::I128(xdr::Int128Parts {
                    hi: (amount_in as i128 >> 64) as i64,
                    lo: amount_in as u64,
                }),
                price_limit,
                hints_val.clone(),
            ];

            let chain_out = match rpc.simulate_call(pool_addr, "swap", swap_args).await {
                Ok(result) => parse_sushi_swap_result(&result, true),
                Err(e) => extract_transfer_amount_from_error(&e.to_string(), true),
            };

            if let Some(chain) = chain_out {
                let diff = local_out.abs_diff(chain);
                let diff_pct = if chain > 0 {
                    (diff as f64 / chain as f64) * 100.0
                } else {
                    0.0
                };
                total_tested += 1;
                let status = if diff_pct < 0.01 {
                    total_matched += 1;
                    "✅"
                } else if diff_pct < 1.0 {
                    total_matched += 1;
                    "⚠️"
                } else {
                    "❌"
                };
                println!(
                    "  {:>6.0} XLM: local={:>12} chain={:>12} diff={:.6}% {}",
                    xlm_human, local_out, chain, diff_pct, status
                );
            } else {
                println!("  {:>6.0} XLM: local={:>12} chain=? (sim failed)", xlm_human, local_out);
            }
        }
    }

    println!("=== Summary: {}/{} tests matched ===", total_matched, total_tested);
    assert!(total_matched > 0, "At least one test should match");
}

fn extract_transfer_amount_from_error(err_str: &str, _zero_for_one: bool) -> Option<u128> {
    // The error contains escaped JSON with pattern like:
    // \"contract call failed\", transfer, [pool, recipient, AMOUNT]]
    // We need to find the amount in the failed transfer
    // Look for "contract call failed" (with or without escaping)
    let search_patterns = [
        "\"contract call failed\", transfer, [",
        "\\\"contract call failed\\\", transfer, [",
        "contract call failed\", transfer, [",
    ];

    for pattern in search_patterns {
        if let Some(idx) = err_str.find(pattern) {
            let after = &err_str[idx + pattern.len()..];
            // Find the closing bracket
            if let Some(bracket_end) = after.find(']') {
                let segment = &after[..bracket_end];
                // Split by comma, last element is the amount
                let parts: Vec<&str> = segment.split(',').collect();
                if let Some(amount_str) = parts.last() {
                    let cleaned = amount_str.trim().trim_matches(|c: char| !c.is_ascii_digit());
                    if let Ok(amount) = cleaned.parse::<u128>() {
                        // Skip if amount equals input (1000000000) - means input transfer failed
                        if amount == 1000000000 {
                            return None;
                        }
                        return Some(amount);
                    }
                }
            }
        }
    }
    None
}

#[ignore] // requires network
async fn test_sushi_pool_state_reading() {
    let rpc = mainnet_rpc();

    println!("=== Sushi V3 Pool State Reading ===\n");

    // Find XLM/USDC pool (try 3000 bps = 0.3%)
    let token_a_hash = stellar_strkey::Contract::from_string(XLM_CONTRACT).unwrap().0;
    let token_b_hash = stellar_strkey::Contract::from_string(USDC_CONTRACT).unwrap().0;

    let args = vec![
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_a_hash)))),
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_b_hash)))),
        xdr::ScVal::U32(3000),
    ];

    let pool_addr = match rpc.simulate_call(SUSHI_FACTORY, "get_pool", args).await {
        Ok(result) => match scval_to_address(&result) {
            Ok(addr) if !addr.is_empty() => addr,
            _ => {
                println!("No XLM/USDC 0.3% pool found");
                return;
            }
        },
        Err(e) => {
            println!("Factory query failed: {}", e);
            return;
        }
    };

    println!("Pool: {}", pool_addr);

    // Read slot0
    match rpc.call_no_args(&pool_addr, "slot0").await {
        Ok(result) => {
            println!("slot0: {:?}", result);
        }
        Err(e) => println!("slot0 failed: {}", e),
    }

    // Read liquidity
    match rpc.call_no_args(&pool_addr, "liquidity").await {
        Ok(result) => {
            if let Ok(liq) = scval_to_u128(&result) {
                println!("liquidity: {}", liq);
            } else {
                println!("liquidity raw: {:?}", result);
            }
        }
        Err(e) => println!("liquidity failed: {}", e),
    }

    // Read fee
    match rpc.call_no_args(&pool_addr, "fee").await {
        Ok(result) => println!("fee: {:?}", result),
        Err(e) => println!("fee failed: {}", e),
    }

    // Read tick_spacing
    match rpc.call_no_args(&pool_addr, "tick_spacing").await {
        Ok(result) => println!("tick_spacing: {:?}", result),
        Err(e) => println!("tick_spacing failed: {}", e),
    }

    // Read token0, token1
    match rpc.call_no_args(&pool_addr, "token0").await {
        Ok(result) => {
            if let Ok(addr) = scval_to_address(&result) {
                println!("token0: {}", addr);
            }
        }
        Err(e) => println!("token0 failed: {}", e),
    }
    match rpc.call_no_args(&pool_addr, "token1").await {
        Ok(result) => {
            if let Ok(addr) = scval_to_address(&result) {
                println!("token1: {}", addr);
            }
        }
        Err(e) => println!("token1 failed: {}", e),
    }
}

/// Parse Sushi SwapResult from simulate response.
/// Result is wrapped: Vec [Symbol("Ok"), SwapResult_map]
/// SwapResult: Map { amount0: i128, amount1: i128, liquidity: u128,
/// sqrt_price_x96: U256, tick: i32 } Returns the output amount (positive
/// value).
fn parse_sushi_swap_result(val: &xdr::ScVal, zero_for_one: bool) -> Option<u128> {
    // Try unwrapping Result::Ok wrapper
    let inner = match val {
        xdr::ScVal::Vec(Some(vec)) if vec.0.len() >= 2 => {
            // Vec [Symbol("Ok"), inner_val]
            &vec.0[1]
        }
        _ => val,
    };

    if let xdr::ScVal::Map(Some(map)) = inner {
        let mut amount0: Option<i128> = None;
        let mut amount1: Option<i128> = None;

        for entry in map.0.iter() {
            let key_name = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key_name.as_str() {
                "amount0" => {
                    amount0 = scval_to_i128(&entry.val).ok();
                }
                "amount1" => {
                    amount1 = scval_to_i128(&entry.val).ok();
                }
                _ => {}
            }
        }

        if let (Some(a0), Some(a1)) = (amount0, amount1) {
            // In exact-input swap: positive = user pays, negative = user receives
            let output = if zero_for_one { -a1 } else { -a0 };
            if output > 0 {
                return Some(output as u128);
            }
        }
    }
    None
}

fn parse_sushi_slot0(val: &xdr::ScVal) -> Option<(dex_adapters::clmm_math::U256, i32)> {
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut sqrt_price = None;
        let mut tick = None;
        for entry in map.0.iter() {
            let key = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key.as_str() {
                "sqrt_price_x96" => {
                    if let xdr::ScVal::U256(parts) = &entry.val {
                        sqrt_price = Some(dex_adapters::clmm_math::U256([
                            parts.lo_lo,
                            parts.lo_hi,
                            parts.hi_lo,
                            parts.hi_hi,
                        ]));
                    }
                }
                "tick" => {
                    if let xdr::ScVal::I32(v) = &entry.val {
                        tick = Some(*v);
                    }
                }
                _ => {}
            }
        }
        if let (Some(sp), Some(t)) = (sqrt_price, tick) {
            return Some((sp, t));
        }
    }
    None
}

fn parse_populated_tick_for_test(val: &xdr::ScVal) -> Option<(i32, u128, i128)> {
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut tick = None;
        let mut lg = None;
        let mut ln = None;
        for entry in map.0.iter() {
            let key = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key.as_str() {
                "tick" => {
                    if let xdr::ScVal::I32(v) = &entry.val {
                        tick = Some(*v);
                    }
                }
                "liquidity_gross" => {
                    lg = scval_to_u128(&entry.val).ok();
                }
                "liquidity_net" => {
                    ln = scval_to_i128(&entry.val).ok();
                }
                _ => {}
            }
        }
        if let (Some(t), Some(g), Some(n)) = (tick, lg, ln) {
            return Some((t, g, n));
        }
    }
    None
}

fn floor_div(a: i32, b: i32) -> i32 {
    let d = a / b;
    if (a ^ b) < 0 && d * b != a {
        d - 1
    } else {
        d
    }
}

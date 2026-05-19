//! Sushi V3 adapter: Concentrated Liquidity AMM on Soroban.
//!
//! TVL: ~$1.95M (per DeFiLlama).
//! Contracts:
//!   Router: CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL
//!   Factory: CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF
//!   Pool Lens: CDFGDFKEN7EVMI3DKIEQ6BKDAKEPHTEPWC6G2ZTDY7ATVCLD24AAU2IN
//!
//! Quote approach: local CLMM tick math (shared with Aquarius concentrated).
//! Reads pool state via simulate_call on pool contract (slot0, liquidity, tick_spacing).
//! Reads tick data via pool-lens get_populated_ticks_in_word.
//!
//! Sushi V3 storage layout (from contract-bindings):
//!   - Slot0: { sqrt_price_x96: U256, tick: i32 }
//!   - TickBitmap(i32): U256 — standard Uniswap V3 bitmap (1 bit per compressed tick)
//!   - Tick(i32): TickInfo { liquidity_gross, liquidity_net, ... }
//!   - Bitmap word_pos = compressed_tick / 256
//!   - compressed_tick = tick / tick_spacing (floor division)

use crate::clmm_math::{self, bitmap, ClmmPoolState, TickDataStore, TickState, U256 as ClmmU256, TICKS_PER_CHUNK};
use crate::rpc::{SorobanRpc, scval_to_address, scval_to_i128, scval_to_u128};
use crate::traits::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// Sushi V3 Router contract address on Stellar Mainnet
pub const SUSHI_ROUTER: &str = "CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL";

/// Sushi V3 Factory contract address on Stellar Mainnet
pub const SUSHI_FACTORY: &str = "CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF";

/// Sushi V3 Pool Lens contract (for batch tick reading)
pub const SUSHI_POOL_LENS: &str = "CDFGDFKEN7EVMI3DKIEQ6BKDAKEPHTEPWC6G2ZTDY7ATVCLD24AAU2IN";

/// Known Sushi V3 fee tiers (basis points)
const FEE_TIERS: &[u32] = &[100, 500, 3000, 10000]; // 0.01%, 0.05%, 0.3%, 1%

/// How many bitmap words to scan around the current tick (each direction)
/// Dynamically adjusted based on tick_spacing:
/// - spacing=10: each word covers 2560 ticks, need more words for wide ranges
/// - spacing=60: each word covers 15360 ticks, fewer words needed
/// - spacing=200: each word covers 51200 ticks, very few words needed
fn bitmap_scan_words(tick_spacing: i32) -> i32 {
    match tick_spacing {
        1..=10 => 30,    // 30 words × 256 × 10 = 76,800 ticks each direction
        11..=60 => 15,   // 15 words × 256 × 60 = 230,400 ticks each direction
        _ => 10,         // 10 words × 256 × 200 = 512,000 ticks each direction
    }
}

/// Cached Sushi pool state for local quoting.
#[derive(Debug, Clone)]
struct SushiPoolCache {
    pool_address: String,
    token0: String,
    token1: String,
    fee_bps: u32,
    tick_spacing: i32,
    sqrt_price_x96: ClmmU256,
    tick: i32,
    liquidity: u128,
    tick_store: TickDataStore,
}

pub struct SushiAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
    pool_cache: RwLock<HashMap<String, SushiPoolCache>>,
}

impl SushiAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
            pool_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Read pool state from the pool contract via simulate_call.
    async fn read_pool_state(&self, pool_address: &str) -> Result<SushiPoolCache> {
        // slot0() -> Map { sqrt_price_x96: U256, tick: i32 }
        let slot0_val = self.rpc.call_no_args(pool_address, "slot0").await
            .map_err(|e| anyhow!("slot0 failed: {}", e))?;
        let (sqrt_price_x96, tick) = parse_slot0(&slot0_val)?;

        // liquidity() -> u128
        let liq_val = self.rpc.call_no_args(pool_address, "liquidity").await
            .map_err(|e| anyhow!("liquidity failed: {}", e))?;
        let liquidity = scval_to_u128(&liq_val)
            .map_err(|e| anyhow!("Cannot parse liquidity: {}", e))?;

        // fee() -> u32
        let fee_val = self.rpc.call_no_args(pool_address, "fee").await
            .map_err(|e| anyhow!("fee failed: {}", e))?;
        let fee_bps = match &fee_val {
            xdr::ScVal::U32(v) => *v,
            _ => 3000,
        };

        // tick_spacing() -> i32
        let ts_val = self.rpc.call_no_args(pool_address, "tick_spacing").await
            .map_err(|e| anyhow!("tick_spacing failed: {}", e))?;
        let tick_spacing = match &ts_val {
            xdr::ScVal::I32(v) => *v,
            _ => 60,
        };

        // token0() -> Address
        let t0_val = self.rpc.call_no_args(pool_address, "token0").await
            .map_err(|e| anyhow!("token0 failed: {}", e))?;
        let token0 = scval_to_address(&t0_val)
            .map_err(|e| anyhow!("Cannot parse token0: {}", e))?;

        // token1() -> Address
        let t1_val = self.rpc.call_no_args(pool_address, "token1").await
            .map_err(|e| anyhow!("token1 failed: {}", e))?;
        let token1 = scval_to_address(&t1_val)
            .map_err(|e| anyhow!("Cannot parse token1: {}", e))?;

        Ok(SushiPoolCache {
            pool_address: pool_address.to_string(),
            token0,
            token1,
            fee_bps,
            tick_spacing,
            sqrt_price_x96,
            tick,
            liquidity,
            tick_store: TickDataStore::new(),
        })
    }

    /// Read tick data via pool-lens get_populated_ticks_in_word.
    /// Scans bitmap words around the current tick.
    async fn read_tick_data(&self, pool: &mut SushiPoolCache) -> Result<()> {
        // Sushi V3 bitmap: word_pos = compressed_tick / 256
        // compressed_tick = tick / tick_spacing (floor division)
        let compressed_tick = floor_div(pool.tick, pool.tick_spacing);
        let current_word = floor_div(compressed_tick, 256);

        let pool_hash = stellar_strkey::Contract::from_string(&pool.pool_address)
            .map_err(|e| anyhow!("Invalid pool address: {:?}", e))?.0;
        let pool_addr_val = xdr::ScVal::Address(
            xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash)))
        );

        // Scan words around current tick (dynamic range based on tick_spacing)
        let scan_words = bitmap_scan_words(pool.tick_spacing);
        let start_word = current_word - scan_words;
        let end_word = current_word + scan_words;

        for word_pos in start_word..=end_word {
            let args = vec![
                pool_addr_val.clone(),
                xdr::ScVal::I32(word_pos),
            ];

            match self.rpc.simulate_call(SUSHI_POOL_LENS, "get_populated_ticks_in_word", args).await {
                Ok(result) => {
                    if let xdr::ScVal::Vec(Some(ticks_vec)) = &result {
                        for tick_val in ticks_vec.0.iter() {
                            if let Some((tick_idx, lg, ln)) = parse_populated_tick(tick_val) {
                                if lg > 0 {
                                    // Store in our TickDataStore using Aquarius-style chunked format
                                    let compressed = bitmap::compress_tick(tick_idx, pool.tick_spacing);
                                    let (chunk_pos, slot) = bitmap::chunk_address(compressed);

                                    let chunk = pool.tick_store.chunks
                                        .entry(chunk_pos)
                                        .or_insert_with(|| vec![TickState { liquidity_gross: 0, liquidity_net: 0 }; TICKS_PER_CHUNK as usize]);
                                    chunk[slot as usize] = TickState { liquidity_gross: lg, liquidity_net: ln };

                                    // Also set bitmap bit
                                    let (bm_word, bm_bit) = bitmap::chunk_bitmap_position(chunk_pos);
                                    let word = pool.tick_store.chunk_bitmap
                                        .entry(bm_word)
                                        .or_insert([0u8; 32]);
                                    let byte_idx = 31 - (bm_bit / 8) as usize;
                                    let bit_idx = bm_bit % 8;
                                    word[byte_idx] |= 1u8 << bit_idx;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Sushi get_populated_ticks_in_word({}) failed: {}", word_pos, e);
                }
            }
        }

        let loaded_ticks: usize = pool.tick_store.chunks.values()
            .map(|c| c.iter().filter(|t| t.liquidity_gross > 0).count())
            .sum();
        debug!("Sushi {}: loaded {} initialized ticks from {} bitmap words",
            pool.pool_address, loaded_ticks, end_word - start_word + 1);

        Ok(())
    }

    /// Get a local CLMM quote.
    fn local_quote(&self, pool: &SushiPoolCache, token_in: &str, amount_in: u128) -> Option<u128> {
        if pool.liquidity == 0 {
            return None;
        }

        let zero_for_one = token_in == pool.token0;

        // Sushi V3 fee is in ppm (parts per million): 3000 = 0.3%
        // Our clmm_math uses FEE_DENOMINATOR = 10_000 (basis points)
        // Convert: fee_bps = fee_ppm / 100
        // But actually clmm_math::FEE_DENOMINATOR is 10_000 and fee_pips is in that unit
        // Sushi fee_ppm: 3000 means 3000/1_000_000 = 0.3%
        // In our math: fee_bps should give same ratio: fee/FEE_DENOMINATOR = 0.003
        // So fee_bps = 30 (30/10000 = 0.3%)
        // Convert: sushi_fee_ppm / 100 = our fee_bps
        let fee_bps_for_math = pool.fee_bps / 100; // 3000 -> 30, 500 -> 5, 10000 -> 100

        let pool_state = ClmmPoolState {
            sqrt_price_x96: pool.sqrt_price_x96,
            tick: pool.tick,
            liquidity: pool.liquidity,
            fee_bps: fee_bps_for_math,
            tick_spacing: pool.tick_spacing,
            token0: pool.token0.clone(),
            token1: pool.token1.clone(),
        };

        let result = clmm_math::simulate_swap(&pool_state, &pool.tick_store, amount_in, zero_for_one);
        result.map(|(amount_out, _, _)| amount_out)
    }

    /// Discover pools by scanning Factory contract events for pool creation.
    /// Falls back to brute-force token pair enumeration if events fail.
    async fn discover_pools(&self) -> Result<Vec<AdapterTradingPair>> {
        // Primary: check hardcoded known pool addresses (fastest, most reliable)
        let pools = self.check_known_pools().await;
        if !pools.is_empty() {
            info!("Sushi: found {} pools from known addresses", pools.len());
            return Ok(pools);
        }

        // Fallback 1: stellar.expert API
        match self.discover_pools_from_factory_storage().await {
            Ok(pools) if !pools.is_empty() => {
                info!("Sushi: discovered {} pools from factory storage", pools.len());
                return Ok(pools);
            }
            _ => {}
        }

        // Fallback 2: brute-force
        self.discover_pools_brute_force().await
    }

    /// Discover all pools by reading the Factory's contract storage.
    /// The Factory stores pool addresses under GetPool(token0, token1, fee) keys.
    /// We read all storage entries and extract unique pool addresses.
    async fn discover_pools_from_factory_storage(&self) -> Result<Vec<AdapterTradingPair>> {
        let client = reqwest::Client::new();
        let url = format!(
            "https://api.stellar.expert/explorer/public/contract-data/{}?limit=200",
            SUSHI_FACTORY
        );

        let resp = client.get(&url).send().await
            .map_err(|e| anyhow!("stellar.expert request failed: {}", e))?;
        let data: serde_json::Value = resp.json().await
            .map_err(|e| anyhow!("stellar.expert response parse failed: {}", e))?;

        let records = data.get("_embedded")
            .and_then(|e| e.get("records"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow!("No records in response"))?;

        // Extract unique pool addresses from storage values
        let mut pool_hashes: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
        for record in records {
            let val_b64 = match record.get("value").and_then(|v| v.as_str()) {
                Some(v) => v,
                None => continue,
            };

            // Decode XDR: ScVal::Address(Contract(hash))
            // Format: 00000012 00000001 <32 bytes hash>
            use base64::Engine;
            let raw = match base64::engine::general_purpose::STANDARD.decode(val_b64) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if raw.len() >= 40 && raw[3] == 0x12 && raw[7] == 0x01 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&raw[8..40]);
                pool_hashes.insert(hash);
            }
        }

        info!("Sushi: found {} unique pool addresses from factory storage", pool_hashes.len());

        // Convert hashes to strkey addresses and read pool info concurrently
        let mut pools = Vec::new();
        let pool_addrs: Vec<String> = pool_hashes.iter()
            .map(|hash| format!("{}", stellar_strkey::Contract(*hash)))
            .collect();

        // Read pool info in batches of 10 (concurrent)
        for chunk in pool_addrs.chunks(10) {
            let futures: Vec<_> = chunk.iter().map(|pool_addr| {
                let rpc = self.rpc.clone();
                let addr = pool_addr.clone();
                async move {
                    // Read token0, token1, fee, liquidity concurrently
                    let (t0, t1, fee_res, liq) = tokio::join!(
                        rpc.call_no_args(&addr, "token0"),
                        rpc.call_no_args(&addr, "token1"),
                        rpc.call_no_args(&addr, "fee"),
                        rpc.call_no_args(&addr, "liquidity"),
                    );

                    let token0 = t0.ok().and_then(|v| scval_to_address(&v).ok())?;
                    let token1 = t1.ok().and_then(|v| scval_to_address(&v).ok())?;
                    let fee = match fee_res.ok()? { xdr::ScVal::U32(f) => f, _ => return None };
                    let liquidity = liq.ok().and_then(|v| scval_to_u128(&v).ok()).unwrap_or(0);

                    if liquidity > 0 {
                        Some(AdapterTradingPair {
                            token_a: TokenId::Contract { address: token0 },
                            token_b: TokenId::Contract { address: token1 },
                            pool_address: addr,
                            fee_bps: fee,
                            reserve_a: None,
                            reserve_b: None,
                        })
                    } else {
                        None
                    }
                }
            }).collect();

            let results = futures::future::join_all(futures).await;
            for result in results {
                if let Some(pair) = result {
                    pools.push(pair);
                }
            }
        }

        Ok(pools)
    }

    /// Fallback: brute-force discovery by trying known token pairs.
    async fn discover_pools_brute_force(&self) -> Result<Vec<AdapterTradingPair>> {
        // First try hardcoded pool addresses (fastest, always works)
        let known_pools = self.check_known_pools().await;
        if !known_pools.is_empty() {
            return Ok(known_pools);
        }

        // Then try token pair enumeration
        let tokens = vec![
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA", // XLM
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", // EURC
            "CAP5AMC2OHNVREO66DFIN6DHJMPOBAJ2KCDDIMFBR7WWJH5RZBFM3UEI", // AQUA
            "CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM", // BLND
            "CBZVSNVB55ANF3LBFTU2LKGD3BJKFMHIGISKND7LBSPHYY3MAQH4AMPR", // yXLM
            "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4MZLQO346H4GQ2O2", // FIDR
            "CCGIMRMF6XGCXBPFY3OIAFAHD24HO5MBNHPFMHBHCNDS2AIMYQCL7PSI", // SHX
        ];

        let mut pools = Vec::new();
        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                for &fee in FEE_TIERS {
                    match self.get_pool_address(tokens[i], tokens[j], fee).await {
                        Ok(Some(pool_addr)) => {
                            pools.push(AdapterTradingPair {
                                token_a: TokenId::Contract { address: tokens[i].to_string() },
                                token_b: TokenId::Contract { address: tokens[j].to_string() },
                                pool_address: pool_addr,
                                fee_bps: fee,
                                reserve_a: None,
                                reserve_b: None,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => debug!("Sushi pool discovery error: {}", e),
                    }
                }
            }
        }

        info!("Sushi: discovered {} pools via brute-force", pools.len());
        Ok(pools)
    }

    /// Check hardcoded pool addresses (from factory storage dump).
    /// This is the fastest discovery method — just verify each pool has liquidity.
    async fn check_known_pools(&self) -> Vec<AdapterTradingPair> {
        // Pool addresses with known liquidity (from sushi.com/stellar/explore/pools)
        // Only include pools that had TVL > $40 on the website
        const KNOWN_POOL_ADDRS: &[&str] = &[
            "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ", // XLM/USDC 0.3% - $660k TVL
            "CAKWXQDEVVUF2ABUEM3M2G7QJGJNDZNNVXJZYG4Z4QP6K54QTWV4DW2S", // CETES/USDC 0.05%
            "CAWWOFOEGWPPNP6QKVHTJYB7UHRXC6W6EAFMUPGHMJL7K46E6UCOSNDM", // PYUSD/USDC 0.05%
            "CAXJ2FDV6S3L46EFEFRXUBLQ5U5CZLZOG35RPCJRNQVLM5MH2HCK5I7J", // USDY/USDC 0.05%
            "CABMZD6BYKKLHRJNS5MURYOBX77NPAH767AI7EVFGWV3WZV55QFN5YNE", // TESOURO/USDC 0.05%
            "CAFLJXGUAURAMBA3AIHC7ZJOAQKGZ7WEFFGMH5XRC35IMNU7PWIBXVTP", // USDGLO/USDC 0.05%
            "CA75VVHLWSM7W6ULNQI7ZJYDFOMQCCPKIDDDHBAL5KOKHWWKWQ5S7MHO", // DAWG/XLM 1%
            "CAUBW4ARD42U2UEIA7GDUB5LNKTRTVYJHXKL3CV27YZRDFADDGKLZWFD", // LIBRE/XLM 0.05%
            "CCRKQ2RHBWB5ZCHOSBSYEC2QNVSU3MGVUF56BWWKJMJIJ3ZF2A6W7KEC", // ACT/XLM 0.3%
            "CBVKO35SAF2ZT75FCLCGLYQG3S6B32YZTOJ2G5F7M746UGBRAWZ5BNZ6", // ACT/MBC 0.3%
            "CAPT5THGW7WOCX47TICCB5JZZK4Y24CHQIBSM57Y472WFFV6FGTRKJQD", // Apay/XLM 0.3%
            "CAWN3BM2ADBMA4CQZLIHTBXA3BQHV4VAPK42LWT5ONAKZW6PH2BBCKLS", // LIBRE/USDC 1%
            "CALM7JTAJC7AJ7ZGTQKXZNNILJUCD2AZNN7QA7FVM3YYIJBCJGUABEDH", // MBC/XLM 0.3%
        ];

        info!("Sushi: checking {} known pool addresses...", KNOWN_POOL_ADDRS.len());
        let mut pools = Vec::new();

        // Use public RPC for discovery (server's local RPC may have issues with some contracts)
        let discovery_rpc = SorobanRpc::new(
            "https://soroban-rpc.mainnet.stellar.gateway.fm",
            "Public Global Stellar Network ; September 2015",
        );

        // Check pools sequentially
        for &addr in KNOWN_POOL_ADDRS {
            let token0 = match discovery_rpc.call_no_args(addr, "token0").await {
                Ok(v) => match scval_to_address(&v) { Ok(a) => a, Err(_) => continue },
                Err(e) => {
                    debug!("Sushi: pool {} token0 failed: {}", &addr[..12], e);
                    continue;
                }
            };
            let token1 = match discovery_rpc.call_no_args(addr, "token1").await {
                Ok(v) => match scval_to_address(&v) { Ok(a) => a, Err(_) => continue },
                Err(_) => continue,
            };
            let fee = match discovery_rpc.call_no_args(addr, "fee").await {
                Ok(xdr::ScVal::U32(f)) => f,
                _ => continue,
            };
            let liquidity = match discovery_rpc.call_no_args(addr, "liquidity").await {
                Ok(v) => scval_to_u128(&v).unwrap_or(0),
                Err(_) => continue,
            };

            if liquidity > 0 {
                pools.push(AdapterTradingPair {
                    token_a: TokenId::Contract { address: token0 },
                    token_b: TokenId::Contract { address: token1 },
                    pool_address: addr.to_string(),
                    fee_bps: fee,
                    reserve_a: None,
                    reserve_b: None,
                });
            }
        }

        info!("Sushi: {} pools with liquidity from known addresses", pools.len());
        pools
    }

    /// Query factory for a pool address given token pair and fee.
    async fn get_pool_address(&self, token_a: &str, token_b: &str, fee: u32) -> Result<Option<String>> {
        let token_a_hash = stellar_strkey::Contract::from_string(token_a)
            .map_err(|e| anyhow!("Invalid token: {:?}", e))?.0;
        let token_b_hash = stellar_strkey::Contract::from_string(token_b)
            .map_err(|e| anyhow!("Invalid token: {:?}", e))?.0;

        let args = vec![
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_a_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_b_hash)))),
            xdr::ScVal::U32(fee),
        ];

        match self.rpc.simulate_call(SUSHI_FACTORY, "get_pool", args).await {
            Ok(result) => {
                match scval_to_address(&result) {
                    Ok(addr) if !addr.is_empty() && !addr.contains("AAAAAAAAAAAAA") => Ok(Some(addr)),
                    _ => Ok(None),
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Simulate quote as fallback (slow but always works).
    async fn simulate_quote_fallback(
        &self,
        token_in: &str,
        token_out: &str,
        amount_in: u128,
        fee_bps: u32,
    ) -> Result<Option<u128>> {
        let token_in_hash = stellar_strkey::Contract::from_string(token_in)
            .map_err(|e| anyhow!("Invalid token_in: {:?}", e))?.0;
        let token_out_hash = stellar_strkey::Contract::from_string(token_out)
            .map_err(|e| anyhow!("Invalid token_out: {:?}", e))?.0;

        let amount_in_val = xdr::ScVal::I128(xdr::Int128Parts {
            hi: (amount_in as i128 >> 64) as i64,
            lo: amount_in as u64,
        });
        let min_out_val = xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 0 });
        let deadline_val = xdr::ScVal::U64(1779031769);

        let path_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
        ].try_into().unwrap())));

        let fees_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![
            xdr::ScVal::U32(fee_bps),
        ].try_into().unwrap())));

        let dummy_addr = xdr::ScVal::Address(xdr::ScAddress::Account(
            xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([0u8; 32])))
        ));
        let checkpoints_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![].try_into().unwrap())));

        let args = vec![
            amount_in_val, min_out_val, deadline_val, path_val, fees_val,
            dummy_addr.clone(), dummy_addr, checkpoints_val,
        ];

        match self.rpc.simulate_call(SUSHI_ROUTER, "swap_exact_input_hints", args).await {
            Ok(result) => {
                if let Ok(amount_out) = scval_to_i128(&result) {
                    return Ok(Some(amount_out as u128));
                }
                if let Ok(amount_out) = scval_to_u128(&result) {
                    return Ok(Some(amount_out));
                }
                Ok(None)
            }
            Err(e) => {
                debug!("Sushi simulate fallback failed: {}", e);
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl DexAdapter for SushiAdapter {
    fn id(&self) -> &str {
        "sushi"
    }

    fn name(&self) -> &str {
        "Sushi V3"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanAmm
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let pairs = self.discover_pools().await?;

        // Load pool state + tick data for each discovered pool
        let mut cache = HashMap::new();
        for pair in &pairs {
            match self.read_pool_state(&pair.pool_address).await {
                Ok(mut pool) => {
                    // Read tick data via pool-lens
                    if let Err(e) = self.read_tick_data(&mut pool).await {
                        warn!("Sushi: failed to read tick data for {}: {}", pair.pool_address, e);
                        // Still store the pool (can fall back to simulate)
                    }
                    cache.insert(pair.pool_address.clone(), pool);
                }
                Err(e) => {
                    warn!("Sushi: failed to read pool state for {}: {}", pair.pool_address, e);
                }
            }
        }

        info!("Sushi: loaded state for {} pools", cache.len());
        *self.pairs.write().await = pairs.clone();
        *self.pool_cache.write().await = cache;
        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let cache = self.pool_cache.read().await;

        // Try local CLMM quote first
        if let Some(pool) = cache.get(pool_address) {
            if let Some(amount_out) = self.local_quote(pool, &token_in.canonical(), amount_in) {
                // Estimate price impact from liquidity
                let impact_bps = if pool.liquidity > 0 {
                    ((amount_in as f64 / pool.liquidity as f64) * 10_000.0).min(10_000.0) as u32
                } else { 0 };
                return Ok(Some(AdapterQuote {
                    amount_out,
                    fee_bps: pool.fee_bps,
                    price_impact_bps: impact_bps,
                }));
            }
        }
        drop(cache);

        // Fall back to simulate
        let pairs = self.pairs.read().await;
        let pair = match pairs.iter().find(|p| p.pool_address == pool_address) {
            Some(p) => p,
            None => return Ok(None),
        };
        let fee_bps = pair.fee_bps;
        drop(pairs);

        let token_in_addr = token_in.canonical();
        let token_out_addr = token_out.canonical();

        match self.simulate_quote_fallback(&token_in_addr, &token_out_addr, amount_in, fee_bps).await? {
            Some(amount_out) if amount_out > 0 => {
                Ok(Some(AdapterQuote {
                    amount_out,
                    fee_bps,
                    price_impact_bps: 0,
                }))
            }
            _ => Ok(None),
        }
    }

    async fn build_swap_op(
        &self,
        _token_in: &TokenId,
        _token_out: &TokenId,
        _amount_in: u128,
        _min_amount_out: u128,
        _pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::SorobanInvoke {
            contract_id: SUSHI_ROUTER.to_string(),
            function_name: "swap_exact_input_hints".to_string(),
            args_xdr: vec![],
        })
    }

    async fn health_check(&self) -> bool {
        self.rpc.call_no_args(SUSHI_FACTORY, "get_protocol_fee_0").await.is_ok()
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        // Re-read slot0 + liquidity for all cached pools
        let pool_addresses: Vec<String> = self.pool_cache.read().await.keys().cloned().collect();
        let mut updated = 0;

        for addr in &pool_addresses {
            // Read slot0 and liquidity
            let slot0_val = match self.rpc.call_no_args(addr, "slot0").await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let liq_val = match self.rpc.call_no_args(addr, "liquidity").await {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let (Ok((sqrt_price, tick)), Ok(liquidity)) = (parse_slot0(&slot0_val), scval_to_u128(&liq_val)) {
                let mut cache = self.pool_cache.write().await;
                if let Some(pool) = cache.get_mut(addr) {
                    pool.sqrt_price_x96 = sqrt_price;
                    pool.tick = tick;
                    pool.liquidity = liquidity;
                    updated += 1;
                }
            }
        }

        Ok(updated)
    }

    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        self.pairs.read().await.clone()
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Floor division (rounds toward negative infinity).
fn floor_div(a: i32, b: i32) -> i32 {
    let d = a / b;
    if (a ^ b) < 0 && d * b != a {
        d - 1
    } else {
        d
    }
}

/// Parse Slot0 from ScVal: Map { sqrt_price_x96: U256, tick: i32 }
fn parse_slot0(val: &xdr::ScVal) -> Result<(ClmmU256, i32)> {
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut sqrt_price = None;
        let mut tick = None;

        for entry in map.0.iter() {
            let key_name = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key_name.as_str() {
                "sqrt_price_x96" => {
                    sqrt_price = parse_u256_scval(&entry.val);
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
            return Ok((sp, t));
        }
    }
    Err(anyhow!("Cannot parse Slot0"))
}

/// Parse U256 from ScVal (supports U256Parts format from simulate_call).
fn parse_u256_scval(val: &xdr::ScVal) -> Option<ClmmU256> {
    match val {
        xdr::ScVal::U256(parts) => {
            // UInt256Parts { hi_hi, hi_lo, lo_hi, lo_lo }
            // Our U256 is little-endian limbs: [lo_lo, lo_hi, hi_lo, hi_hi]
            Some(ClmmU256([parts.lo_lo, parts.lo_hi, parts.hi_lo, parts.hi_hi]))
        }
        xdr::ScVal::U128(parts) => {
            let v = (parts.hi as u128) << 64 | parts.lo as u128;
            Some(ClmmU256::from_u128(v))
        }
        _ => None,
    }
}

/// Parse PopulatedTick from ScVal: Map { tick: i32, liquidity_gross: u128, liquidity_net: i128 }
fn parse_populated_tick(val: &xdr::ScVal) -> Option<(i32, u128, i128)> {
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut tick = None;
        let mut lg = None;
        let mut ln = None;

        for entry in map.0.iter() {
            let key_name = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key_name.as_str() {
                "tick" => {
                    if let xdr::ScVal::I32(v) = &entry.val { tick = Some(*v); }
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

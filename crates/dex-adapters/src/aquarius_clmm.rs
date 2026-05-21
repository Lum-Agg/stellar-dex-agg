//! Aquarius Concentrated Liquidity (CLMM) adapter.
//!
//! Reads pool state (Slot0, Liquidity, tick chunks, bitmaps) from chain via
//! `getLedgerEntries` and computes swap quotes locally using `clmm_math`.
//!
//! Storage layout (from thirdparty/aquarius-amm/liquidity_pool_concentrated/src/storage.rs):
//! - Instance storage:
//!   - DataKey::Slot0 -> Slot0 { sqrt_price_x96: U256, tick: i32 }
//!   - DataKey::Liquidity -> u128
//!   - DataKey::Fee -> u32 (basis points, e.g. 30 = 0.3%)
//!   - DataKey::TickSpacing -> i32
//!   - DataKey::Token0 -> Address
//!   - DataKey::Token1 -> Address
//!   - DataKey::MinInitTick -> i32
//!   - DataKey::MaxInitTick -> i32
//! - Persistent storage:
//!   - DataKey::TickChunk(i32) -> Vec<TickData(U256, U256, u128, i128)>
//!   - DataKey::ChunkBitmap(i32) -> U256
//!   - DataKey::WordBitmap(i32) -> U256

use crate::clmm_math::{
    self, bitmap, clmm_pool_to_snapshot, loaded_tick_range, ClmmPoolState, TickDataStore, TickState,
    TICKS_PER_CHUNK, U256 as ClmmU256,
};
use crate::rpc::SorobanRpc;
use crate::traits::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use market_snapshot::{ClmmCoverageSnapshot, ClmmPoolSnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Aquarius concentrated pool state (cached for quoting).
#[derive(Debug, Clone)]
pub struct AquaClmmPool {
    pub pool_address: String,
    pub token0: String,
    pub token1: String,
    pub fee_bps: u32,
    pub tick_spacing: i32,
    pub sqrt_price_x96: ClmmU256,
    pub tick: i32,
    pub liquidity: u128,
    pub min_init_tick: i32,
    pub max_init_tick: i32,
    pub tick_store: TickDataStore,
}

pub struct AquariusClmmAdapter {
    rpc: Arc<SorobanRpc>,
    /// Known concentrated pool addresses (discovered from router or hardcoded)
    pool_addresses: RwLock<Vec<String>>,
    /// Cached pool states for local quoting
    pools: RwLock<HashMap<String, AquaClmmPool>>,
    /// Trading pairs derived from pools
    pairs: RwLock<Vec<AdapterTradingPair>>,
}

impl AquariusClmmAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pool_addresses: RwLock::new(Vec::new()),
            pools: RwLock::new(HashMap::new()),
            pairs: RwLock::new(Vec::new()),
        }
    }

    fn snapshot_pool(pool: &AquaClmmPool) -> ClmmPoolSnapshot {
        let pool_state = ClmmPoolState {
            sqrt_price_x96: pool.sqrt_price_x96,
            tick: pool.tick,
            liquidity: pool.liquidity,
            fee_bps: pool.fee_bps,
            tick_spacing: pool.tick_spacing,
            token0: pool.token0.clone(),
            token1: pool.token1.clone(),
        };
        clmm_pool_to_snapshot(
            "aquarius_clmm",
            pool.pool_address.clone(),
            &pool_state,
            &pool.tick_store,
            Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(pool.min_init_tick),
                max_loaded_tick: Some(pool.max_init_tick),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        )
    }

    pub async fn export_clmm_snapshots(&self) -> Vec<ClmmPoolSnapshot> {
        let pools = self.pools.read().await;
        let mut snapshots = pools
            .values()
            .filter(|pool| loaded_tick_range(&pool.tick_store, pool.tick_spacing).is_some())
            .map(Self::snapshot_pool)
            .collect::<Vec<_>>();
        snapshots.sort_by(|a, b| a.pool_address.cmp(&b.pool_address));
        snapshots
    }

    /// Set known concentrated pool addresses (discovered externally or hardcoded).
    pub async fn set_pool_addresses(&self, addresses: Vec<String>) {
        *self.pool_addresses.write().await = addresses;
    }

    /// Discover all concentrated pools from the Aquarius router.
    /// Queries get_tokens_sets_count() then iterates get_pools_for_tokens_range(),
    /// filtering for pools where pool_type() == "concentrated".
    async fn discover_concentrated_pools(&self) -> Result<Vec<String>> {
        use crate::aquarius::AQUARIUS_ROUTER;
        use crate::rpc::scval_to_u128;

        // 1. Get total token sets count
        let count_val = self
            .rpc
            .call_no_args(AQUARIUS_ROUTER, "get_tokens_sets_count")
            .await?;
        let total_count = scval_to_u128(&count_val)?;
        info!("Aquarius CLMM: total token sets = {}", total_count);

        if total_count == 0 {
            return Ok(vec![]);
        }

        // 2. Fetch all pools in batches
        let batch_size: u128 = 50;
        let mut all_pool_addresses = Vec::new();
        let mut start: u128 = 0;

        while start < total_count {
            let end = (start + batch_size).min(total_count);

            let start_val = xdr::ScVal::U128(xdr::UInt128Parts {
                hi: (start >> 64) as u64,
                lo: start as u64,
            });
            let end_val = xdr::ScVal::U128(xdr::UInt128Parts {
                hi: (end >> 64) as u64,
                lo: end as u64,
            });

            match self
                .rpc
                .simulate_call(
                    AQUARIUS_ROUTER,
                    "get_pools_for_tokens_range",
                    vec![start_val, end_val],
                )
                .await
            {
                Ok(result) => {
                    // Parse pool addresses from result
                    if let xdr::ScVal::Vec(Some(entries)) = &result {
                        for entry in entries.0.iter() {
                            if let xdr::ScVal::Vec(Some(pair)) = entry {
                                if pair.0.len() >= 2 {
                                    if let xdr::ScVal::Map(Some(map)) = &pair.0[1] {
                                        for map_entry in map.0.iter() {
                                            if let Ok(addr) =
                                                crate::rpc::scval_to_address(&map_entry.val)
                                            {
                                                all_pool_addresses.push(addr);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Aquarius CLMM: batch [{}, {}) failed: {}", start, end, e);
                }
            }

            start = end;
            if start < total_count {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }

        info!(
            "Aquarius CLMM: found {} total pool addresses, filtering for concentrated...",
            all_pool_addresses.len()
        );

        // 3. Filter for concentrated pools by calling pool_type()
        let mut concentrated = Vec::new();
        for chunk in all_pool_addresses.chunks(20) {
            let futures: Vec<_> = chunk
                .iter()
                .map(|addr| {
                    let rpc = self.rpc.clone();
                    let addr = addr.clone();
                    async move {
                        match rpc.call_no_args(&addr, "pool_type").await {
                            Ok(xdr::ScVal::Symbol(s)) => {
                                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                                if name == "concentrated" {
                                    Some(addr)
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(futures).await;
            for result in results {
                if let Some(addr) = result {
                    concentrated.push(addr);
                }
            }
        }

        info!(
            "Aquarius CLMM: found {} concentrated pools",
            concentrated.len()
        );
        Ok(concentrated)
    }

    /// Read pool instance storage to get Slot0, Liquidity, Fee, TickSpacing, Token0, Token1.
    /// Uses simulate_call on individual getter functions (more reliable than raw storage parsing).
    async fn read_pool_instance(&self, pool_address: &str) -> Result<AquaClmmPool> {
        // Read via contract function calls (works regardless of XDR version issues)
        // Aquarius concentrated pool has: get_tokens(), get_fee_fraction(), get_reserves()
        // and we can read Slot0 fields via estimate_swap with 0 amount

        // get_tokens() -> Vec<Address>
        let tokens_val = self.rpc.call_no_args(pool_address, "get_tokens").await?;
        let (token0, token1) = match &tokens_val {
            xdr::ScVal::Vec(Some(vec)) if vec.0.len() >= 2 => {
                let t0 = crate::rpc::scval_to_address(&vec.0[0])?;
                let t1 = crate::rpc::scval_to_address(&vec.0[1])?;
                (t0, t1)
            }
            _ => return Err(anyhow!("Cannot parse get_tokens result")),
        };

        // get_fee_fraction() -> u32
        let fee_val = self
            .rpc
            .call_no_args(pool_address, "get_fee_fraction")
            .await?;
        let fee_bps = match &fee_val {
            xdr::ScVal::U32(v) => *v,
            _ => 30, // default
        };

        // get_info() -> Map with tick_spacing
        let info_val = self.rpc.call_no_args(pool_address, "get_info").await?;
        let tick_spacing = extract_tick_spacing_from_info(&info_val).unwrap_or(200);

        // Read Slot0 via getLedgerEntries on instance storage
        // If that fails, try to infer from simulate
        let (sqrt_price_x96, tick, liquidity) = match self.read_slot0_via_ledger(pool_address).await
        {
            Ok(state) => state,
            Err(_) => {
                // Fallback: read via get_pool_state_with_balances or similar
                match self.read_slot0_via_simulate(pool_address).await {
                    Ok(state) => state,
                    Err(e) => return Err(anyhow!("Cannot read pool state: {}", e)),
                }
            }
        };

        Ok(AquaClmmPool {
            pool_address: pool_address.to_string(),
            token0,
            token1,
            fee_bps,
            tick_spacing,
            sqrt_price_x96,
            tick,
            liquidity,
            min_init_tick: clmm_math::MIN_TICK,
            max_init_tick: clmm_math::MAX_TICK,
            tick_store: TickDataStore::new(),
        })
    }

    /// Try to read Slot0 from instance storage via getLedgerEntries.
    async fn read_slot0_via_ledger(&self, pool_address: &str) -> Result<(ClmmU256, i32, u128)> {
        let contract_hash = stellar_strkey::Contract::from_string(pool_address)
            .map_err(|e| anyhow!("Invalid contract address: {:?}", e))?
            .0;

        let instance_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
            contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
            key: xdr::ScVal::LedgerKeyContractInstance,
            durability: xdr::ContractDataDurability::Persistent,
        });

        let entries = self.rpc.get_ledger_entries(vec![instance_key]).await?;
        if entries.is_empty() {
            return Err(anyhow!("No instance data"));
        }

        let instance_map = parse_instance_storage(&entries[0].entry.data)?;
        let sqrt_price_x96 = extract_slot0_sqrt_price(&instance_map)?;
        let tick = extract_slot0_tick(&instance_map)?;
        let liquidity = extract_u128_field(&instance_map, "Liquidity")?;

        Ok((sqrt_price_x96, tick, liquidity))
    }

    /// Fallback: read pool state by simulating get_reserves and inferring from estimate_swap.
    async fn read_slot0_via_simulate(&self, pool_address: &str) -> Result<(ClmmU256, i32, u128)> {
        // Use get_slot0() and get_active_liquidity()
        let slot0_val = self
            .rpc
            .call_no_args(pool_address, "get_slot0")
            .await
            .map_err(|e| anyhow!("get_slot0 failed: {}", e))?;

        let (sqrt_price_x96, tick) = parse_slot0_scval(&slot0_val)?;

        let liq_val = self
            .rpc
            .call_no_args(pool_address, "get_active_liquidity")
            .await
            .map_err(|e| anyhow!("get_active_liquidity failed: {}", e))?;
        let liquidity = crate::rpc::scval_to_u128(&liq_val)
            .map_err(|e| anyhow!("Cannot parse liquidity: {}", e))?;

        Ok((sqrt_price_x96, tick, liquidity))
    }

    /// Read tick chunks and bitmaps from persistent storage for a pool.
    /// Uses simulate_call on pool's getter functions (avoids XDR decode issues with U256).
    async fn read_tick_data(&self, pool: &mut AquaClmmPool) -> Result<()> {
        // 1. Read tick bounds
        let bounds_val = self
            .rpc
            .call_no_args(&pool.pool_address, "get_tick_bounds")
            .await?;
        if let xdr::ScVal::Vec(Some(vec)) = &bounds_val {
            if vec.0.len() >= 2 {
                if let xdr::ScVal::I32(min_t) = &vec.0[0] {
                    pool.min_init_tick = *min_t;
                }
                if let xdr::ScVal::I32(max_t) = &vec.0[1] {
                    pool.max_init_tick = *max_t;
                }
            }
        }

        debug!(
            "Aquarius CLMM {}: tick bounds [{}, {}]",
            pool.pool_address, pool.min_init_tick, pool.max_init_tick
        );

        if pool.min_init_tick >= pool.max_init_tick {
            // Pool has no initialized ticks (empty)
            return Ok(());
        }

        // 2. Read chunk bitmap via get_chunk_bitmap_batch(start_word, count)
        let min_compressed = bitmap::compress_tick(pool.min_init_tick, pool.tick_spacing);
        let max_compressed = bitmap::compress_tick(pool.max_init_tick, pool.tick_spacing);
        let (min_chunk, _) = bitmap::chunk_address(min_compressed);
        let (max_chunk, _) = bitmap::chunk_address(max_compressed);
        let min_bm_word = min_chunk >> 8;
        let max_bm_word = max_chunk >> 8;
        let bm_word_count = (max_bm_word - min_bm_word + 1) as u32;

        let start_word_val = xdr::ScVal::I32(min_bm_word);
        let count_val = xdr::ScVal::U32(bm_word_count.min(50)); // Cap at 50 words

        match self
            .rpc
            .simulate_call(
                &pool.pool_address,
                "get_chunk_bitmap_batch",
                vec![start_word_val, count_val],
            )
            .await
        {
            Ok(result) => {
                if let xdr::ScVal::Vec(Some(vec)) = &result {
                    for (i, val) in vec.0.iter().enumerate() {
                        let word_pos = min_bm_word + i as i32;
                        if let Some(u256) = parse_u256_from_scval_any(val) {
                            let bytes = u256_to_be_bytes(&u256);
                            pool.tick_store.chunk_bitmap.insert(word_pos, bytes);
                        }
                    }
                }
            }
            Err(e) => {
                debug!("get_chunk_bitmap_batch failed: {}", e);
            }
        }

        // 3. Find all initialized ticks from bitmap and read them via get_ticks_batch
        let mut initialized_ticks = Vec::new();
        for chunk_pos in min_chunk..=max_chunk {
            let (bm_word, bm_bit) = bitmap::chunk_bitmap_position(chunk_pos);
            if let Some(word) = pool.tick_store.chunk_bitmap.get(&bm_word) {
                let byte_idx = 31 - (bm_bit / 8) as usize;
                let bit_idx = bm_bit % 8;
                if (word[byte_idx] >> bit_idx) & 1 == 1 {
                    // This chunk has initialized ticks — we need to find which ones
                    // For each slot in the chunk, compute the actual tick
                    for slot in 0..TICKS_PER_CHUNK {
                        let compressed = chunk_pos * TICKS_PER_CHUNK + slot;
                        let actual_tick = bitmap::compressed_to_tick(compressed, pool.tick_spacing);
                        if actual_tick >= pool.min_init_tick && actual_tick <= pool.max_init_tick {
                            initialized_ticks.push(actual_tick);
                        }
                    }
                }
            }
        }

        if initialized_ticks.is_empty() {
            return Ok(());
        }

        debug!(
            "Aquarius CLMM {}: reading {} candidate ticks",
            pool.pool_address,
            initialized_ticks.len()
        );

        // 4. Read ticks in batches via get_ticks_batch
        for batch in initialized_ticks.chunks(50) {
            let ticks_vec: Vec<xdr::ScVal> = batch.iter().map(|t| xdr::ScVal::I32(*t)).collect();
            let ticks_val = xdr::ScVal::Vec(Some(xdr::ScVec(ticks_vec.try_into().unwrap())));

            match self
                .rpc
                .simulate_call(&pool.pool_address, "get_ticks_batch", vec![ticks_val])
                .await
            {
                Ok(result) => {
                    if let xdr::ScVal::Vec(Some(vec)) = &result {
                        for (i, tick_info_val) in vec.0.iter().enumerate() {
                            if i >= batch.len() {
                                break;
                            }
                            let tick_idx = batch[i];
                            if let Some((lg, ln)) = parse_tick_info_scval(tick_info_val) {
                                if lg > 0 {
                                    // Store in our tick data store
                                    let compressed =
                                        bitmap::compress_tick(tick_idx, pool.tick_spacing);
                                    let (chunk_pos, slot) = bitmap::chunk_address(compressed);

                                    let chunk =
                                        pool.tick_store.chunks.entry(chunk_pos).or_insert_with(
                                            || {
                                                vec![
                                                    TickState {
                                                        liquidity_gross: 0,
                                                        liquidity_net: 0
                                                    };
                                                    TICKS_PER_CHUNK as usize
                                                ]
                                            },
                                        );
                                    chunk[slot as usize] = TickState {
                                        liquidity_gross: lg,
                                        liquidity_net: ln,
                                    };
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("get_ticks_batch failed: {}", e);
                }
            }

            // Rate limit
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let loaded_ticks: usize = pool
            .tick_store
            .chunks
            .values()
            .map(|c| c.iter().filter(|t| t.liquidity_gross > 0).count())
            .sum();
        debug!(
            "Aquarius CLMM {}: loaded {} initialized ticks",
            pool.pool_address, loaded_ticks
        );

        Ok(())
    }

    /// Get a local quote using the CLMM math.
    fn local_quote(&self, pool: &AquaClmmPool, token_in: &str, amount_in: u128) -> Option<u128> {
        let zero_for_one = token_in == pool.token0;

        let pool_state = ClmmPoolState {
            sqrt_price_x96: pool.sqrt_price_x96,
            tick: pool.tick,
            liquidity: pool.liquidity,
            fee_bps: pool.fee_bps,
            tick_spacing: pool.tick_spacing,
            token0: pool.token0.clone(),
            token1: pool.token1.clone(),
        };

        let result =
            clmm_math::simulate_swap(&pool_state, &pool.tick_store, amount_in, zero_for_one);
        result.map(|(amount_out, _, _)| amount_out)
    }
}

#[async_trait]
impl DexAdapter for AquariusClmmAdapter {
    fn id(&self) -> &str {
        "aquarius_clmm"
    }

    fn name(&self) -> &str {
        "Aquarius Concentrated"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanAmm
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        // Auto-discover concentrated pools from Aquarius router
        let mut addresses = self.pool_addresses.read().await.clone();

        if addresses.is_empty() {
            info!("Aquarius CLMM: auto-discovering concentrated pools from router...");
            addresses = self.discover_concentrated_pools().await?;
            *self.pool_addresses.write().await = addresses.clone();
        }

        if addresses.is_empty() {
            info!("Aquarius CLMM: no concentrated pools found");
            return Ok(vec![]);
        }

        info!(
            "Aquarius CLMM: loading state for {} concentrated pools...",
            addresses.len()
        );

        let mut all_pairs = Vec::new();
        let mut all_pools = HashMap::new();

        for addr in &addresses {
            match self.read_pool_instance(addr).await {
                Ok(mut pool) => {
                    // Read tick data
                    if let Err(e) = self.read_tick_data(&mut pool).await {
                        warn!(
                            "Aquarius CLMM: failed to read tick data for {}: {}",
                            addr, e
                        );
                        continue;
                    }

                    let pair = AdapterTradingPair {
                        token_a: TokenId::Contract {
                            address: pool.token0.clone(),
                        },
                        token_b: TokenId::Contract {
                            address: pool.token1.clone(),
                        },
                        pool_address: addr.clone(),
                        fee_bps: pool.fee_bps,
                        reserve_a: None,
                        reserve_b: None,
                    };
                    all_pairs.push(pair);
                    all_pools.insert(addr.clone(), pool);
                }
                Err(e) => {
                    warn!("Aquarius CLMM: failed to read pool {}: {}", addr, e);
                }
            }
        }

        info!("Aquarius CLMM: loaded {} pools", all_pools.len());
        *self.pools.write().await = all_pools;
        *self.pairs.write().await = all_pairs.clone();
        Ok(all_pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        _token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let pools = self.pools.read().await;
        let pool = match pools.get(pool_address) {
            Some(p) => p,
            None => return Ok(None),
        };

        let token_in_addr = token_in.canonical();
        if token_in_addr != pool.token0 && token_in_addr != pool.token1 {
            return Ok(None);
        }

        match self.local_quote(pool, &token_in_addr, amount_in) {
            Some(amount_out) if amount_out > 0 => {
                Ok(Some(AdapterQuote {
                    amount_out,
                    fee_bps: pool.fee_bps,
                    price_impact_bps: 0, // Complex for CLMM, skip
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
        pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::SorobanInvoke {
            contract_id: pool_address.to_string(),
            function_name: "swap".to_string(),
            args_xdr: vec![],
        })
    }

    async fn health_check(&self) -> bool {
        let addresses = self.pool_addresses.read().await;
        if addresses.is_empty() {
            return false;
        }
        // Try reading the first pool's instance
        self.read_pool_instance(&addresses[0]).await.is_ok()
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        // Re-read Slot0 + Liquidity for all pools (fast: just instance storage)
        let addresses = self.pool_addresses.read().await.clone();
        let mut updated = 0;

        for addr in &addresses {
            match self.read_pool_instance(addr).await {
                Ok(new_pool) => {
                    let mut pools = self.pools.write().await;
                    if let Some(pool) = pools.get_mut(addr) {
                        pool.sqrt_price_x96 = new_pool.sqrt_price_x96;
                        pool.tick = new_pool.tick;
                        pool.liquidity = new_pool.liquidity;
                        updated += 1;
                    }
                }
                Err(_) => {}
            }
        }

        Ok(updated)
    }

    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        self.pairs.read().await.clone()
    }
}

// ============================================================================
// XDR Parsing Helpers
// ============================================================================

/// Variants of the DataKey enum for building ledger keys.
enum DataKeyVariant {
    TickChunk(i32),
    ChunkBitmap(i32),
    WordBitmap(i32),
}

/// Build a persistent storage ledger key for a given contract and data key variant.
fn make_persistent_key(contract_hash: &[u8; 32], variant: &DataKeyVariant) -> xdr::LedgerKey {
    let key_val = match variant {
        DataKeyVariant::TickChunk(pos) => {
            // DataKey::TickChunk(i32) is enum variant index 23 (counting from storage.rs)
            // In XDR, Soroban enums with data are encoded as Vec [symbol, val]
            xdr::ScVal::Vec(Some(xdr::ScVec(
                vec![
                    xdr::ScVal::Symbol(xdr::ScSymbol("TickChunk".try_into().unwrap())),
                    xdr::ScVal::I32(*pos),
                ]
                .try_into()
                .unwrap(),
            )))
        }
        DataKeyVariant::ChunkBitmap(pos) => xdr::ScVal::Vec(Some(xdr::ScVec(
            vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("ChunkBitmap".try_into().unwrap())),
                xdr::ScVal::I32(*pos),
            ]
            .try_into()
            .unwrap(),
        ))),
        DataKeyVariant::WordBitmap(pos) => xdr::ScVal::Vec(Some(xdr::ScVec(
            vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("WordBitmap".try_into().unwrap())),
                xdr::ScVal::I32(*pos),
            ]
            .try_into()
            .unwrap(),
        ))),
    };

    xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
        contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(*contract_hash))),
        key: key_val,
        durability: xdr::ContractDataDurability::Persistent,
    })
}

/// Parse instance storage from a ledger entry into a map of field name -> ScVal.
fn parse_instance_storage(entry: &xdr::LedgerEntryData) -> Result<HashMap<String, xdr::ScVal>> {
    let mut map = HashMap::new();

    if let xdr::LedgerEntryData::ContractData(data) = entry {
        if let xdr::ScVal::ContractInstance(instance) = &data.val {
            if let Some(storage) = &instance.storage {
                for item in storage.0.iter() {
                    let key_name = scval_to_symbol_name(&item.key);
                    if let Some(name) = key_name {
                        map.insert(name, item.val.clone());
                    }
                }
            }
        }
    }

    Ok(map)
}

fn scval_to_symbol_name(val: &xdr::ScVal) -> Option<String> {
    match val {
        xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).ok(),
        // Enum variant without data: Vec [Symbol(name)]
        xdr::ScVal::Vec(Some(vec)) if !vec.0.is_empty() => {
            if let xdr::ScVal::Symbol(s) = &vec.0[0] {
                String::from_utf8(s.0.to_vec()).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_slot0_sqrt_price(map: &HashMap<String, xdr::ScVal>) -> Result<ClmmU256> {
    let slot0_val = map
        .get("Slot0")
        .ok_or_else(|| anyhow!("No Slot0 in instance"))?;
    // Slot0 is a struct: { sqrt_price_x96: U256, tick: i32 }
    // In XDR it's a Map with symbol keys
    if let xdr::ScVal::Map(Some(m)) = slot0_val {
        for entry in m.0.iter() {
            if let xdr::ScVal::Symbol(s) = &entry.key {
                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                if name == "sqrt_price_x96" {
                    return parse_u256_scval(&entry.val);
                }
            }
        }
    }
    Err(anyhow!("Could not parse Slot0.sqrt_price_x96"))
}

fn extract_slot0_tick(map: &HashMap<String, xdr::ScVal>) -> Result<i32> {
    let slot0_val = map
        .get("Slot0")
        .ok_or_else(|| anyhow!("No Slot0 in instance"))?;
    if let xdr::ScVal::Map(Some(m)) = slot0_val {
        for entry in m.0.iter() {
            if let xdr::ScVal::Symbol(s) = &entry.key {
                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                if name == "tick" {
                    if let xdr::ScVal::I32(v) = &entry.val {
                        return Ok(*v);
                    }
                }
            }
        }
    }
    Err(anyhow!("Could not parse Slot0.tick"))
}

fn extract_u128_field(map: &HashMap<String, xdr::ScVal>, name: &str) -> Result<u128> {
    let val = map
        .get(name)
        .ok_or_else(|| anyhow!("No {} in instance", name))?;
    match val {
        xdr::ScVal::U128(parts) => Ok((parts.hi as u128) << 64 | parts.lo as u128),
        xdr::ScVal::I128(parts) => Ok((parts.hi as u128) << 64 | parts.lo as u128),
        xdr::ScVal::U64(v) => Ok(*v as u128),
        xdr::ScVal::U32(v) => Ok(*v as u128),
        _ => Err(anyhow!("Cannot parse {} as u128", name)),
    }
}

fn extract_u32_field(map: &HashMap<String, xdr::ScVal>, name: &str) -> Result<u32> {
    let val = map
        .get(name)
        .ok_or_else(|| anyhow!("No {} in instance", name))?;
    match val {
        xdr::ScVal::U32(v) => Ok(*v),
        xdr::ScVal::U64(v) => Ok(*v as u32),
        _ => Err(anyhow!("Cannot parse {} as u32", name)),
    }
}

fn extract_i32_field(map: &HashMap<String, xdr::ScVal>, name: &str) -> Result<i32> {
    let val = map
        .get(name)
        .ok_or_else(|| anyhow!("No {} in instance", name))?;
    match val {
        xdr::ScVal::I32(v) => Ok(*v),
        xdr::ScVal::I64(v) => Ok(*v as i32),
        _ => Err(anyhow!("Cannot parse {} as i32", name)),
    }
}

fn extract_address_field(map: &HashMap<String, xdr::ScVal>, name: &str) -> Result<String> {
    let val = map
        .get(name)
        .ok_or_else(|| anyhow!("No {} in instance", name))?;
    crate::rpc::scval_to_address(val)
        .map_err(|e| anyhow!("Cannot parse {} as address: {}", name, e))
}

fn extract_tick_spacing_from_info(val: &xdr::ScVal) -> Option<i32> {
    if let xdr::ScVal::Map(Some(map)) = val {
        for entry in map.0.iter() {
            if let xdr::ScVal::Symbol(s) = &entry.key {
                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                if name == "tick_spacing" {
                    if let xdr::ScVal::I32(v) = &entry.val {
                        return Some(*v);
                    }
                }
            }
        }
    }
    None
}

/// Parse Slot0 from simulate_call result: Map { sqrt_price_x96: U256, tick: i32 }
fn parse_slot0_scval(val: &xdr::ScVal) -> Result<(ClmmU256, i32)> {
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
                    sqrt_price = parse_u256_from_scval_any(&entry.val);
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
    Err(anyhow!(
        "Cannot parse Slot0 from {:?}",
        std::mem::discriminant(val)
    ))
}

/// Parse U256 from any ScVal format (U256Parts, Bytes, etc.)
fn parse_u256_from_scval_any(val: &xdr::ScVal) -> Option<ClmmU256> {
    match val {
        xdr::ScVal::U256(parts) => {
            // UInt256Parts { hi_hi, hi_lo, lo_hi, lo_lo }
            // Our U256 is little-endian limbs: [lo_lo, lo_hi, hi_lo, hi_hi]
            Some(ClmmU256([
                parts.lo_lo,
                parts.lo_hi,
                parts.hi_lo,
                parts.hi_hi,
            ]))
        }
        xdr::ScVal::Bytes(bytes) if bytes.0.len() == 32 => {
            let b = &bytes.0;
            let limb0 =
                u64::from_be_bytes([b[24], b[25], b[26], b[27], b[28], b[29], b[30], b[31]]);
            let limb1 =
                u64::from_be_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]);
            let limb2 = u64::from_be_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
            let limb3 = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
            Some(ClmmU256([limb0, limb1, limb2, limb3]))
        }
        xdr::ScVal::U128(parts) => {
            let v = (parts.hi as u128) << 64 | parts.lo as u128;
            Some(ClmmU256::from_u128(v))
        }
        _ => None,
    }
}

/// Convert our U256 to big-endian 32 bytes (for bitmap storage).
fn u256_to_be_bytes(val: &ClmmU256) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    // limbs[3] is most significant (big-endian first)
    bytes[0..8].copy_from_slice(&val.0[3].to_be_bytes());
    bytes[8..16].copy_from_slice(&val.0[2].to_be_bytes());
    bytes[16..24].copy_from_slice(&val.0[1].to_be_bytes());
    bytes[24..32].copy_from_slice(&val.0[0].to_be_bytes());
    bytes
}

/// Parse TickInfo from ScVal (Map with symbol keys).
/// Returns (liquidity_gross, liquidity_net) or None.
fn parse_tick_info_scval(val: &xdr::ScVal) -> Option<(u128, i128)> {
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut lg = None;
        let mut ln = None;
        for entry in map.0.iter() {
            let key_name = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };
            match key_name.as_str() {
                "liquidity_gross" => {
                    lg = parse_u128_scval(&entry.val);
                }
                "liquidity_net" => {
                    ln = parse_i128_scval(&entry.val);
                }
                _ => {}
            }
        }
        if let (Some(g), Some(n)) = (lg, ln) {
            return Some((g, n));
        }
    }
    None
}

/// Parse pool state from get_pool_state_with_balances result.
fn parse_pool_state_with_balances(val: &xdr::ScVal) -> Result<(ClmmU256, i32, u128)> {
    // Expected format: struct { reserve0, reserve1, state: { fee, liquidity, sqrt_price_x96, tick, ... } }
    if let xdr::ScVal::Map(Some(map)) = val {
        let mut sqrt_price = None;
        let mut tick = None;
        let mut liquidity = None;

        for entry in map.0.iter() {
            let key_name = match &entry.key {
                xdr::ScVal::Symbol(s) => String::from_utf8(s.0.to_vec()).unwrap_or_default(),
                _ => continue,
            };

            match key_name.as_str() {
                "liquidity" => {
                    liquidity = parse_u128_scval(&entry.val);
                }
                "sqrt_price_x96" => {
                    sqrt_price = parse_u256_scval(&entry.val).ok();
                }
                "tick" => {
                    if let xdr::ScVal::I32(v) = &entry.val {
                        tick = Some(*v);
                    }
                }
                "state" => {
                    // Nested state struct
                    if let xdr::ScVal::Map(Some(inner)) = &entry.val {
                        for inner_entry in inner.0.iter() {
                            let inner_key = match &inner_entry.key {
                                xdr::ScVal::Symbol(s) => {
                                    String::from_utf8(s.0.to_vec()).unwrap_or_default()
                                }
                                _ => continue,
                            };
                            match inner_key.as_str() {
                                "liquidity" => {
                                    liquidity = parse_u128_scval(&inner_entry.val);
                                }
                                "sqrt_price_x96" => {
                                    sqrt_price = parse_u256_scval(&inner_entry.val).ok();
                                }
                                "tick" => {
                                    if let xdr::ScVal::I32(v) = &inner_entry.val {
                                        tick = Some(*v);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let (Some(sp), Some(t), Some(l)) = (sqrt_price, tick, liquidity) {
            return Ok((sp, t, l));
        }
    }
    Err(anyhow!("Cannot parse pool state"))
}

/// Parse a U256 from ScVal (stored as Bytes<32> big-endian on chain).
fn parse_u256_scval(val: &xdr::ScVal) -> Result<ClmmU256> {
    match val {
        xdr::ScVal::Bytes(bytes) if bytes.0.len() == 32 => {
            // Big-endian bytes -> little-endian limbs
            let b = &bytes.0;
            let limb0 =
                u64::from_be_bytes([b[24], b[25], b[26], b[27], b[28], b[29], b[30], b[31]]);
            let limb1 =
                u64::from_be_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]);
            let limb2 = u64::from_be_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
            let limb3 = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
            Ok(ClmmU256([limb0, limb1, limb2, limb3]))
        }
        xdr::ScVal::U128(parts) => {
            let v = (parts.hi as u128) << 64 | parts.lo as u128;
            Ok(ClmmU256::from_u128(v))
        }
        // Soroban U256 is stored as U256Val which maps to 4 u64 parts
        xdr::ScVal::Map(Some(m)) if m.0.len() == 4 => {
            // Try parsing as {hi_hi, hi_lo, lo_hi, lo_lo}
            let mut parts = [0u64; 4];
            for (i, entry) in m.0.iter().enumerate() {
                if let xdr::ScVal::U64(v) = &entry.val {
                    parts[i] = *v;
                }
            }
            // Map order: hi_hi, hi_lo, lo_hi, lo_lo -> limbs[3], limbs[2], limbs[1], limbs[0]
            Ok(ClmmU256([parts[3], parts[2], parts[1], parts[0]]))
        }
        _ => Err(anyhow!(
            "Cannot parse ScVal as U256: {:?}",
            std::mem::discriminant(val)
        )),
    }
}

/// Parse a U256 from a ledger entry (persistent storage value).
fn parse_u256_entry(entry: &xdr::LedgerEntryData) -> Option<[u8; 32]> {
    if let xdr::LedgerEntryData::ContractData(data) = entry {
        match &data.val {
            xdr::ScVal::Bytes(bytes) if bytes.0.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes.0);
                return Some(arr);
            }
            _ => {}
        }
    }
    None
}

/// Parse a tick chunk entry: Vec<TickData(U256, U256, u128, i128)>
/// We only need liquidity_gross and liquidity_net for swap simulation.
fn parse_tick_chunk_entry(entry: &xdr::LedgerEntryData) -> Option<Vec<TickState>> {
    if let xdr::LedgerEntryData::ContractData(data) = entry {
        if let xdr::ScVal::Vec(Some(vec)) = &data.val {
            let mut ticks = Vec::with_capacity(TICKS_PER_CHUNK as usize);
            for item in vec.0.iter() {
                // Each TickData is a tuple struct: Vec [U256, U256, u128, i128]
                if let xdr::ScVal::Vec(Some(tuple)) = item {
                    if tuple.0.len() >= 4 {
                        let liquidity_gross = parse_u128_scval(&tuple.0[2]).unwrap_or(0);
                        let liquidity_net = parse_i128_scval(&tuple.0[3]).unwrap_or(0);
                        ticks.push(TickState {
                            liquidity_gross,
                            liquidity_net,
                        });
                    } else {
                        ticks.push(TickState {
                            liquidity_gross: 0,
                            liquidity_net: 0,
                        });
                    }
                } else {
                    ticks.push(TickState {
                        liquidity_gross: 0,
                        liquidity_net: 0,
                    });
                }
            }
            // Pad to TICKS_PER_CHUNK if needed
            while ticks.len() < TICKS_PER_CHUNK as usize {
                ticks.push(TickState {
                    liquidity_gross: 0,
                    liquidity_net: 0,
                });
            }
            return Some(ticks);
        }
    }
    None
}

fn parse_u128_scval(val: &xdr::ScVal) -> Option<u128> {
    match val {
        xdr::ScVal::U128(parts) => Some((parts.hi as u128) << 64 | parts.lo as u128),
        xdr::ScVal::U64(v) => Some(*v as u128),
        xdr::ScVal::U32(v) => Some(*v as u128),
        _ => None,
    }
}

fn parse_i128_scval(val: &xdr::ScVal) -> Option<i128> {
    match val {
        xdr::ScVal::I128(parts) => Some((parts.hi as i128) << 64 | parts.lo as u64 as i128),
        xdr::ScVal::I64(v) => Some(*v as i128),
        xdr::ScVal::I32(v) => Some(*v as i128),
        _ => None,
    }
}

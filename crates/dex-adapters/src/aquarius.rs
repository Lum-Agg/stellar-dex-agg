//! Aquarius adapter: AMM on Soroban with both constant-product and stable-swap
//! pools.
//!
//! Key characteristics:
//! - Router contract: CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK
//! - Supports constant product (xy=k) and Curve stable swap
//! - Fee is per-pool (bps via `get_fee_fraction()`, commonly 30 or 100),
//!   fetched as U32 on mainnet
//! - Stable pools use Curve invariant with amplification factor
//! - Pool discovery via get_tokens_sets_count() + get_pools_for_tokens_range()

use {
    crate::{
        rpc::{parse_fee_bps_u32, scval_to_address, scval_to_u128, SorobanRpc},
        traits::*,
    },
    anyhow::Result,
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, sync::Arc},
    stellar_xdr::curr as xdr,
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

/// Aquarius Router contract address (Mainnet)
pub const AQUARIUS_ROUTER: &str = "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK";

const DEFAULT_AQUARIUS_HYDRATE_CONCURRENCY: usize = 16;

fn aquarius_hydrate_concurrency() -> usize {
    std::env::var("AQUARIUS_HYDRATE_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_AQUARIUS_HYDRATE_CONCURRENCY)
        .max(1)
}

/// Default amplification factor for stable pools (unused when `a()` is
/// readable).
const DEFAULT_STABLE_AMP: u128 = 100;

/// Aquarius stableswap supports 2 or 3 tokens (`STABLESWAP_MAX_TOKENS` in
/// router).
const MAX_STABLESWAP_TOKENS: usize = 3;

/// On-chain Aquarius pool state for local quotes (stable + volatile).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AquariusPoolQuoteState {
    pub pool_address: String,
    /// Token addresses in on-chain order (matches `get_reserves()` indices).
    pub tokens: Vec<String>,
    pub reserves: Vec<u128>,
    pub fee_bps: u32,
    pub is_stable: bool,
    pub amp: u128,
}

/// Quote a hop using full pool reserves and stable/volatile math.
pub fn quote_aquarius_pool(
    state: &AquariusPoolQuoteState,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
) -> Option<AdapterQuote> {
    if amount_in == 0 || state.reserves.is_empty() {
        return None;
    }
    let in_idx = state.tokens.iter().position(|t| t == token_in)?;
    let out_idx = state.tokens.iter().position(|t| t == token_out)?;
    if in_idx == out_idx {
        return None;
    }
    let reserve_in = *state.reserves.get(in_idx)?;
    if reserve_in == 0 {
        return None;
    }

    let amount_out = if state.is_stable {
        AquariusAdapter::stable_swap_quote_multi(&state.reserves, in_idx, out_idx, amount_in, state.fee_bps, state.amp)
    } else {
        let reserve_out = *state.reserves.get(out_idx)?;
        AquariusAdapter::constant_product_quote(amount_in, reserve_in, reserve_out, state.fee_bps)
    };

    if amount_out == 0 {
        return None;
    }

    let price_impact_bps = (amount_in * 10_000 / (2 * reserve_in)).min(10_000) as u32;
    Some(AdapterQuote {
        amount_out,
        fee_bps: state.fee_bps,
        price_impact_bps,
    })
}

#[derive(Debug, Clone)]
struct PoolMeta {
    is_stable: bool,
    fee_bps: u32,
    /// All token addresses in order (for multi-token pools)
    all_tokens: Vec<String>,
    /// All reserves in order (for multi-token pools)
    all_reserves: Vec<u128>,
    /// Amplification coefficient
    amp: u128,
}

pub struct AquariusAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
    pool_meta: RwLock<HashMap<String, PoolMeta>>,
}

impl AquariusAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
            pool_meta: RwLock::new(HashMap::new()),
        }
    }

    /// Constant product quote.
    /// Aquarius: in_after_fee = amount_in * (10000 - fee_bps) / 10000
    pub fn constant_product_quote(amount_in: u128, reserve_in: u128, reserve_out: u128, fee_bps: u32) -> u128 {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return 0;
        }
        let in_after_fee = amount_in * (10_000 - fee_bps as u128) / 10_000;
        let numerator = in_after_fee * reserve_out;
        let denominator = reserve_in + in_after_fee;
        numerator / denominator
    }

    /// Curve stable swap quote using stable_math module.
    pub fn stable_swap_quote_multi(
        reserves: &[u128],
        in_idx: usize,
        out_idx: usize,
        amount_in: u128,
        fee_bps: u32,
        amp: u128,
    ) -> u128 {
        use crate::stable_math::StablePool;

        if reserves.is_empty() || in_idx >= reserves.len() || out_idx >= reserves.len() {
            return 0;
        }

        let pool = StablePool {
            reserves: reserves.to_vec(),
            decimals: vec![7; reserves.len()], // All Stellar tokens are 7 decimals
            amp,
            fee_bps,
        };

        pool.get_dy(in_idx, out_idx, amount_in)
    }

    /// Fetch all pools from the Aquarius Router contract.
    async fn fetch_pools_from_router(&self) -> Result<Vec<(AdapterTradingPair, PoolMeta)>> {
        // 1. Get total token sets count
        let count_val = self.rpc.call_no_args(AQUARIUS_ROUTER, "get_tokens_sets_count").await?;
        let total_count = scval_to_u128(&count_val)?;
        info!("Aquarius: total token sets = {}", total_count);

        if total_count == 0 {
            return Ok(vec![]);
        }

        // 2. Collect unique pool addresses from router catalogue.
        let mut pool_addresses = std::collections::HashSet::new();
        let batch_size: u128 = 50;
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
                .simulate_call(AQUARIUS_ROUTER, "get_pools_for_tokens_range", vec![start_val, end_val])
                .await
            {
                Ok(result) => {
                    self.collect_pool_addresses(&result, &mut pool_addresses);
                }
                Err(e) => {
                    warn!("Aquarius batch [{}, {}) failed: {}", start, end, e);
                }
            }

            start = end;

            if start < total_count {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        }

        info!("Aquarius: {} unique pool addresses from router", pool_addresses.len());

        // Drop concentrated pools (quoted/routed via aquarius_clmm).
        let before = pool_addresses.len();
        let mut volatile_addrs = Vec::new();
        for addr in &pool_addresses {
            if self.is_volatile_pool(addr).await {
                volatile_addrs.push(addr.clone());
            }
        }
        pool_addresses = volatile_addrs.into_iter().collect();
        info!(
            "Aquarius: kept {} non-concentrated pools (dropped {} concentrated)",
            pool_addresses.len(),
            before.saturating_sub(pool_addresses.len())
        );

        // 3. Hydrate pools from on-chain get_tokens/get_reserves (supports 2- and
        //    3-token stableswap).
        let pool_list: Vec<String> = pool_addresses.into_iter().collect();
        let hydrate_concurrency = aquarius_hydrate_concurrency();
        info!(
            pools = pool_list.len(),
            concurrency = hydrate_concurrency,
            "Aquarius: hydrating pools via get_tokens + get_reserves..."
        );
        let mut all_pools = Vec::new();
        for chunk in pool_list.chunks(hydrate_concurrency) {
            let results = futures::future::join_all(chunk.iter().map(|pool_addr| self.hydrate_pool(pool_addr))).await;
            for (pool_addr, edges) in chunk.iter().zip(results) {
                match edges {
                    Some(edges) => all_pools.extend(edges),
                    None => {
                        debug!(pool = %pool_addr, "Aquarius pool hydration skipped");
                    }
                }
            }
        }

        info!("Aquarius: fetched {} pool edges with reserves", all_pools.len());
        Ok(all_pools)
    }

    /// Load canonical token list, reserves, and pool type; emit all token-pair
    /// edges.
    async fn hydrate_pool(&self, pool_address: &str) -> Option<Vec<(AdapterTradingPair, PoolMeta)>> {
        let tokens = self.fetch_pool_tokens(pool_address).await.ok()?;
        let n = tokens.len();
        if n < 2 || n > MAX_STABLESWAP_TOKENS {
            return None;
        }

        let is_stable = self.is_stable_pool(pool_address).await;
        if !is_stable && n != 2 {
            warn!(
                pool = %pool_address,
                n_tokens = n,
                "constant_product pool has unexpected token count"
            );
            return None;
        }

        let reserves = self.fetch_pool_reserves_vec(pool_address).await.ok()?;
        if reserves.len() != n || reserves.iter().all(|&r| r == 0) {
            return None;
        }

        let amp = if is_stable {
            self.fetch_pool_amp(pool_address).await
        } else {
            DEFAULT_STABLE_AMP
        };
        let fee_bps = self.fetch_pool_fee_bps(pool_address).await.unwrap_or(30);

        let meta = PoolMeta {
            is_stable,
            fee_bps,
            all_tokens: tokens.clone(),
            all_reserves: reserves.clone(),
            amp,
        };

        let mut edges = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                let reserve_a = reserves.get(i).copied();
                let reserve_b = reserves.get(j).copied();
                if reserve_a.unwrap_or(0) == 0 || reserve_b.unwrap_or(0) == 0 {
                    continue;
                }
                edges.push((
                    AdapterTradingPair {
                        token_a: TokenId::Contract {
                            address: tokens[i].clone(),
                        },
                        token_b: TokenId::Contract {
                            address: tokens[j].clone(),
                        },
                        pool_address: pool_address.to_string(),
                        fee_bps,
                        reserve_a,
                        reserve_b,
                    },
                    meta.clone(),
                ));
            }
        }

        if edges.is_empty() {
            None
        } else {
            Some(edges)
        }
    }

    async fn fetch_pool_tokens(&self, pool_address: &str) -> Result<Vec<String>> {
        let result = self.rpc.call_no_args(pool_address, "get_tokens").await?;
        self.parse_address_vec(&result)
            .ok_or_else(|| anyhow::anyhow!("get_tokens empty for pool {}", pool_address))
    }

    async fn fetch_pool_fee_bps(&self, pool_address: &str) -> Option<u32> {
        match self.rpc.call_no_args(pool_address, "get_fee_fraction").await {
            Ok(val) => parse_fee_bps_u32(&val),
            Err(_) => None,
        }
    }

    /// Extract pool contract addresses from router `get_pools_for_tokens_range`
    /// output.
    fn collect_pool_addresses(&self, val: &xdr::ScVal, out: &mut std::collections::HashSet<String>) {
        let entries = match val {
            xdr::ScVal::Vec(Some(v)) => &v.0,
            _ => return,
        };

        for entry in entries.iter() {
            if let xdr::ScVal::Vec(Some(pair)) = entry {
                if pair.0.len() < 2 {
                    continue;
                }
                if let xdr::ScVal::Map(Some(map)) = &pair.0[1] {
                    for map_entry in map.0.iter() {
                        if let Ok(pool_address) = scval_to_address(&map_entry.val) {
                            out.insert(pool_address);
                        }
                    }
                }
            }
        }
    }

    /// Export one state blob per pool for Redis / quote hydration.
    pub async fn export_pool_quote_states(&self) -> Vec<AquariusPoolQuoteState> {
        let meta_map = self.pool_meta.read().await;
        let mut out = Vec::new();
        for (pool_address, meta) in meta_map.iter() {
            if meta.all_tokens.is_empty() ||
                meta.all_reserves.len() != meta.all_tokens.len() ||
                meta.all_reserves.iter().all(|&r| r == 0)
            {
                continue;
            }
            out.push(AquariusPoolQuoteState {
                pool_address: pool_address.clone(),
                tokens: meta.all_tokens.clone(),
                reserves: meta.all_reserves.clone(),
                fee_bps: meta.fee_bps,
                is_stable: meta.is_stable,
                amp: meta.amp,
            });
        }
        out.sort_by(|a, b| a.pool_address.cmp(&b.pool_address));
        out
    }

    async fn fetch_pool_reserves_vec(&self, pool_address: &str) -> Result<Vec<u128>> {
        let result = self.rpc.call_no_args(pool_address, "get_reserves").await?;
        if let xdr::ScVal::Vec(Some(vec)) = &result {
            let reserves: Vec<u128> = vec.0.iter().filter_map(|v| scval_to_u128(v).ok()).collect();
            if !reserves.is_empty() {
                return Ok(reserves);
            }
        }
        if let xdr::ScVal::Map(Some(map)) = &result {
            let mut reserves = Vec::new();
            for entry in map.0.iter() {
                if let Ok(v) = scval_to_u128(&entry.val) {
                    reserves.push(v);
                }
            }
            if !reserves.is_empty() {
                return Ok(reserves);
            }
        }
        Err(anyhow::anyhow!(
            "Could not parse get_reserves for pool {}",
            pool_address
        ))
    }

    async fn fetch_pool_amp(&self, pool_address: &str) -> u128 {
        match self.rpc.call_no_args(pool_address, "a").await {
            Ok(val) => scval_to_u128(&val).unwrap_or(DEFAULT_STABLE_AMP),
            Err(_) => DEFAULT_STABLE_AMP,
        }
    }

    fn apply_pool_reserves_to_pairs(pairs: &mut [AdapterTradingPair], pool_address: &str, meta: &PoolMeta) {
        for pair in pairs.iter_mut().filter(|p| p.pool_address == pool_address) {
            let a_key = pair.token_a.canonical();
            let b_key = pair.token_b.canonical();
            let Some(i) = meta.all_tokens.iter().position(|t| t == &a_key) else {
                continue;
            };
            let Some(j) = meta.all_tokens.iter().position(|t| t == &b_key) else {
                continue;
            };
            pair.reserve_a = meta.all_reserves.get(i).copied();
            pair.reserve_b = meta.all_reserves.get(j).copied();
        }
    }

    fn parse_address_vec(&self, val: &xdr::ScVal) -> Option<Vec<String>> {
        let mut addrs = Vec::new();
        if let xdr::ScVal::Vec(Some(vec)) = val {
            for item in vec.0.iter() {
                if let Ok(addr) = scval_to_address(item) {
                    addrs.push(addr);
                }
            }
        }
        if addrs.is_empty() {
            None
        } else {
            Some(addrs)
        }
    }

    /// Constant-product / stableswap pools only; concentrated pools use
    /// `aquarius_clmm`.
    async fn is_volatile_pool(&self, pool_address: &str) -> bool {
        match self.rpc.call_no_args(pool_address, "pool_type").await {
            Ok(xdr::ScVal::Symbol(s)) => {
                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                name != "concentrated"
            }
            _ => true,
        }
    }

    async fn is_stable_pool(&self, pool_address: &str) -> bool {
        match self.rpc.call_no_args(pool_address, "pool_type").await {
            Ok(xdr::ScVal::Symbol(s)) => {
                let name = String::from_utf8(s.0.to_vec()).unwrap_or_default();
                name == "stable"
            }
            _ => false,
        }
    }

    /// Batch-refresh reserves via getLedgerEntries (volatile + stableswap
    /// instance storage).
    pub async fn refresh_all_reserves(&self) -> Result<usize> {
        let pool_addresses = self.known_pool_addresses().await;
        self.refresh_pool_addresses(&pool_addresses).await
    }

    pub async fn known_pool_addresses(&self) -> Vec<String> {
        self.pool_meta.read().await.keys().cloned().collect()
    }

    /// Ensure `pool_meta` exists for touched pools (e.g. after restart before
    /// discovery finishes). Missing meta previously made ledger refresh a no-op.
    async fn ensure_pool_meta(&self, pool_addresses: &[String]) {
        let missing: Vec<String> = {
            let meta = self.pool_meta.read().await;
            pool_addresses
                .iter()
                .filter(|addr| !meta.contains_key(addr.as_str()))
                .cloned()
                .collect()
        };
        if missing.is_empty() {
            return;
        }

        for addr in missing {
            let Some(edges) = self.hydrate_pool(&addr).await else {
                warn!(pool = %addr, "Aquarius: hydrate failed for touched pool missing meta");
                continue;
            };
            let Some((_, meta)) = edges.first() else {
                continue;
            };
            let meta = meta.clone();
            let mut meta_map = self.pool_meta.write().await;
            let mut pairs = self.pairs.write().await;
            meta_map.insert(addr.clone(), meta);
            for (pair, _) in edges {
                let exists = pairs.iter().any(|p| {
                    p.pool_address == pair.pool_address &&
                        p.token_a.canonical() == pair.token_a.canonical() &&
                        p.token_b.canonical() == pair.token_b.canonical()
                });
                if !exists {
                    pairs.push(pair);
                }
            }
            debug!(pool = %addr, "Aquarius: hydrated missing pool_meta for ledger touch");
        }
    }

    /// Refresh a subset of pools (used by the fetch pipeline / write-through).
    pub async fn refresh_pool_addresses(&self, pool_addresses: &[String]) -> Result<usize> {
        if pool_addresses.is_empty() {
            return Ok(0);
        }

        self.ensure_pool_meta(pool_addresses).await;

        let concurrency = std::env::var("POOL_STATE_REFRESH_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        let results =
            crate::batch_refresh::batch_refresh_aquarius_reserves_parallel(&self.rpc, pool_addresses, concurrency)
                .await?;

        let mut updated = 0usize;
        let mut meta_map = self.pool_meta.write().await;
        let mut pairs = self.pairs.write().await;

        for (pool_address, reserves) in results {
            let Some(reserves) = reserves else {
                continue;
            };
            let Some(meta) = meta_map.get_mut(&pool_address) else {
                continue;
            };
            if meta.all_tokens.is_empty() || reserves.len() != meta.all_tokens.len() {
                continue;
            }
            meta.all_reserves = reserves;
            Self::apply_pool_reserves_to_pairs(&mut pairs, &pool_address, meta);
            updated += 1;
        }

        debug!("Aquarius: batch-refreshed {}/{} pools", updated, pool_addresses.len());
        Ok(updated)
    }

    /// Export quote states for a subset of pool contracts.
    pub async fn export_pool_quote_states_for(&self, pool_addresses: &[String]) -> Vec<AquariusPoolQuoteState> {
        let wanted: std::collections::HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
        self.export_pool_quote_states()
            .await
            .into_iter()
            .filter(|state| wanted.contains(state.pool_address.as_str()))
            .collect()
    }
}

#[async_trait]
impl DexAdapter for AquariusAdapter {
    fn id(&self) -> &str {
        "aquarius"
    }

    fn name(&self) -> &str {
        "Aquarius"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanAmm
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let results = self.fetch_pools_from_router().await?;

        let mut pairs = Vec::new();
        let mut meta_map = HashMap::new();

        for (pair, meta) in results {
            meta_map.entry(pair.pool_address.clone()).or_insert(meta);
            pairs.push(pair);
        }

        *self.pairs.write().await = pairs.clone();
        *self.pool_meta.write().await = meta_map;

        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let meta_map = self.pool_meta.read().await;
        let meta = meta_map.get(pool_address).cloned().unwrap_or(PoolMeta {
            is_stable: false,
            fee_bps: 30,
            all_tokens: vec![],
            all_reserves: vec![],
            amp: DEFAULT_STABLE_AMP,
        });
        drop(meta_map);

        if meta.all_tokens.len() >= 2 &&
            meta.all_reserves.len() == meta.all_tokens.len() &&
            meta.all_reserves.iter().any(|&r| r > 0)
        {
            let state = AquariusPoolQuoteState {
                pool_address: pool_address.to_string(),
                tokens: meta.all_tokens,
                reserves: meta.all_reserves,
                fee_bps: meta.fee_bps,
                is_stable: meta.is_stable,
                amp: meta.amp,
            };
            return Ok(quote_aquarius_pool(
                &state,
                &token_in.canonical(),
                &token_out.canonical(),
                amount_in,
            ));
        }

        let pairs = self.pairs.read().await;
        let pair = pairs.iter().find(|p| p.pool_address == pool_address);

        let pair = match pair {
            Some(p) => p,
            None => return Ok(None),
        };

        let (reserve_in, reserve_out) = if token_in.canonical() == pair.token_a.canonical() {
            (pair.reserve_a, pair.reserve_b)
        } else if token_in.canonical() == pair.token_b.canonical() {
            (pair.reserve_b, pair.reserve_a)
        } else {
            return Ok(None);
        };

        let reserve_in = match reserve_in {
            Some(r) if r > 0 => r,
            _ => return Ok(None),
        };
        let reserve_out = match reserve_out {
            Some(r) if r > 0 => r,
            _ => return Ok(None),
        };

        let amount_out = if meta.is_stable {
            Self::stable_swap_quote_multi(&[reserve_in, reserve_out], 0, 1, amount_in, meta.fee_bps, meta.amp)
        } else {
            Self::constant_product_quote(amount_in, reserve_in, reserve_out, meta.fee_bps)
        };

        if amount_out == 0 {
            return Ok(None);
        }

        let price_impact_bps = (amount_in * 10_000 / (2 * reserve_in)).min(10_000) as u32;

        Ok(Some(AdapterQuote {
            amount_out,
            fee_bps: meta.fee_bps,
            price_impact_bps,
        }))
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
        self.rpc
            .call_no_args(AQUARIUS_ROUTER, "get_tokens_sets_count")
            .await
            .is_ok()
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        self.refresh_all_reserves().await
    }

    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        self.pairs.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_token_stableswap_matches_router_test_vector() {
        // liquidity_pool_router::test_stableswap_3_pool — equal 100M deposit per coin,
        // amp=6750.
        let reserves = vec![100_0000000u128, 100_0000000, 100_0000000];
        let out = AquariusAdapter::stable_swap_quote_multi(&reserves, 0, 1, 97_0000000, 30, 6750);
        assert_eq!(
            out, 96_5081326,
            "3-token stableswap token0->token1 should match on-chain router test"
        );
    }

    #[test]
    fn three_token_stableswap_quote_via_pool_state() {
        let t0 = "TOKEN_A";
        let t1 = "TOKEN_B";
        let t2 = "TOKEN_C";
        let state = AquariusPoolQuoteState {
            pool_address: "pool3".into(),
            tokens: vec![t0.into(), t1.into(), t2.into()],
            reserves: vec![100_0000000, 100_0000000, 100_0000000],
            fee_bps: 30,
            is_stable: true,
            amp: 6750,
        };
        let hop01 = quote_aquarius_pool(&state, t0, t1, 97_0000000)
            .expect("hop 0->1")
            .amount_out;
        assert_eq!(hop01, 96_5081326);

        // Direct swap across non-adjacent indices uses full n-coin invariant.
        let hop02 = quote_aquarius_pool(&state, t0, t2, 10_0000000)
            .expect("hop 0->2")
            .amount_out;
        assert!(hop02 > 9_9000000 && hop02 < 10_0000000, "hop02={hop02}");
    }

    #[test]
    fn usdc_to_xlm_via_cbvdrt_uses_stable_then_volatile() {
        let usdc = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        let cbvd = "CBVDRT5474OBUEXF5MJB3UGQ5CG7CKGCAH5M4RV5NBCDJUBZ5OXHJLOU";
        let xlm = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

        let hop1 = quote_aquarius_pool(
            &AquariusPoolQuoteState {
                pool_address: "CBRXOYKXPQI4EEA6KA35TUIYN5OJLNWMTIVDOMNOIL2BG5Y5LEDHUU7V".into(),
                tokens: vec![cbvd.into(), usdc.into()],
                reserves: vec![2092266080, 49103367],
                fee_bps: 30,
                is_stable: true,
                amp: 1500,
            },
            usdc,
            cbvd,
            1_480_000,
        )
        .expect("hop1 stable quote")
        .amount_out;

        let hop2 = quote_aquarius_pool(
            &AquariusPoolQuoteState {
                pool_address: "CDYLKM3DGH5A6QA6QOIITPKG7C4DTZMS2HF75XURORACBHCR6AOE3K33".into(),
                tokens: vec![xlm.into(), cbvd.into()],
                reserves: vec![80963820202, 13678306977],
                fee_bps: 30,
                is_stable: false,
                amp: 100,
            },
            cbvd,
            xlm,
            hop1,
        )
        .expect("hop2 volatile quote")
        .amount_out;

        assert!(hop2 > 9_000_000 && hop2 < 11_000_000, "expected ~1 XLM out, got {hop2}");
    }

    #[test]
    fn parse_aquarius_fee_bps_accepts_u32_and_i128() {
        // Mainnet get_fee_fraction returns U32 (e.g. 100 for CCMHVBZG…).
        assert_eq!(parse_fee_bps_u32(&xdr::ScVal::U32(100)), Some(100));
        assert_eq!(parse_fee_bps_u32(&xdr::ScVal::U32(30)), Some(30));
        assert_eq!(
            parse_fee_bps_u32(&xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 100 })),
            Some(100)
        );
        assert!(parse_fee_bps_u32(&xdr::ScVal::Symbol("nope".try_into().unwrap())).is_none());
    }
}

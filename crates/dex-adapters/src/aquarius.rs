//! Aquarius adapter: AMM on Soroban with both constant-product and stable-swap pools.
//!
//! Key characteristics:
//! - Router contract: CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK
//! - Supports constant product (xy=k) and Curve stable swap
//! - Fee is per-pool (typically 10-30 bps), fetched via get_fee_fraction()
//! - Stable pools use Curve invariant with amplification factor
//! - Pool discovery via get_tokens_sets_count() + get_pools_for_tokens_range()

use crate::rpc::{SorobanRpc, scval_to_address, scval_to_u128, scval_to_u32, scval_to_string};
use crate::traits::*;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

/// Aquarius Router contract address (Mainnet)
pub const AQUARIUS_ROUTER: &str = "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK";

/// Known stablecoin assets for detecting stable pools
/// Using SAC contract addresses on mainnet
const STABLE_ASSETS: &[&str] = &[
    "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC SAC
    "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", // EURC SAC
];

/// Default amplification factor for stable pools
const DEFAULT_STABLE_AMP: u128 = 1500;

#[derive(Debug, Clone)]
struct PoolMeta {
    is_stable: bool,
    fee_bps: u32,
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

    /// Curve stable swap quote.
    pub fn stable_swap_quote(amount_in: u128, reserve_in: u128, reserve_out: u128, fee_bps: u32) -> u128 {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return 0;
        }

        let ann = DEFAULT_STABLE_AMP * 4;
        let xp = [reserve_in, reserve_out];

        let d = compute_d(xp, ann);
        if d == 0 {
            return 0;
        }

        let x_new = reserve_in + amount_in;
        let y_new = compute_y(x_new, ann, d);

        if y_new >= reserve_out {
            return 0;
        }

        let dy = reserve_out - y_new - 1;
        let fee = dy * fee_bps as u128 / 10_000;
        dy.saturating_sub(fee)
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

        // 2. Fetch in batches
        let batch_size: u128 = 50;
        let mut all_pools = Vec::new();
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

            match self.rpc
                .simulate_call(AQUARIUS_ROUTER, "get_pools_for_tokens_range", vec![start_val, end_val])
                .await
            {
                Ok(result) => {
                    self.parse_pools_result(&result, &mut all_pools).await;
                }
                Err(e) => {
                    warn!("Aquarius batch [{}, {}) failed: {}", start, end, e);
                }
            }

            start = end;

            // Rate limiting
            if start < total_count {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        }

        // 3. Batch-fetch reserves using getLedgerEntries (fast, 1 RPC call per 200 pools)
        info!("Aquarius: batch-fetching reserves for {} pools...", all_pools.len());
        let pool_addresses: Vec<String> = all_pools.iter().map(|(p, _)| p.pool_address.clone()).collect();
        let batch_results = crate::batch_refresh::batch_refresh_soroswap_reserves(&self.rpc, &pool_addresses).await;

        match batch_results {
            Ok(results) => {
                let found = results.iter().filter(|(_, r)| r.is_some()).count();
                if found > 0 {
                    for (addr, reserves) in &results {
                        if let Some((r0, r1)) = reserves {
                            if let Some((pair, _)) = all_pools.iter_mut().find(|(p, _)| &p.pool_address == addr) {
                                pair.reserve_a = Some(*r0);
                                pair.reserve_b = Some(*r1);
                            }
                        }
                    }
                    info!("Aquarius: batch got reserves for {} pools", found);
                } else {
                    // Batch didn't work (different storage layout), use concurrent simulate
                    info!("Aquarius: batch returned 0, using concurrent simulate...");
                    let batch_size = 50;
                    for chunk in all_pools.chunks_mut(batch_size) {
                        let futures: Vec<_> = chunk.iter().map(|(pair, _)| {
                            self.fetch_pool_reserves(&pair.pool_address)
                        }).collect();
                        let results = futures::future::join_all(futures).await;
                        for (i, result) in results.into_iter().enumerate() {
                            if let Ok((r0, r1)) = result {
                                chunk[i].0.reserve_a = Some(r0);
                                chunk[i].0.reserve_b = Some(r1);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Aquarius batch reserves failed: {}", e);
                // Fallback: concurrent individual calls
                let batch_size = 50;
                for chunk in all_pools.chunks_mut(batch_size) {
                    let futures: Vec<_> = chunk.iter().map(|(pair, _)| {
                        self.fetch_pool_reserves(&pair.pool_address)
                    }).collect();
                    let results = futures::future::join_all(futures).await;
                    for (i, result) in results.into_iter().enumerate() {
                        if let Ok((r0, r1)) = result {
                            chunk[i].0.reserve_a = Some(r0);
                            chunk[i].0.reserve_b = Some(r1);
                        }
                    }
                }
            }
        }

        // Filter out pools with no reserves
        all_pools.retain(|(pair, _)| pair.reserve_a.unwrap_or(0) > 0 || pair.reserve_b.unwrap_or(0) > 0);

        info!("Aquarius: fetched {} pools with reserves", all_pools.len());
        Ok(all_pools)
    }

    /// Fetch reserves for a single Aquarius pool by calling get_reserves on the pool contract.
    async fn fetch_pool_reserves(&self, pool_address: &str) -> Result<(u128, u128)> {
        // Aquarius pools have a get_reserves() function that returns Vec<u128>
        let result = self.rpc.call_no_args(pool_address, "get_reserves").await?;

        if let xdr::ScVal::Vec(Some(vec)) = &result {
            if vec.0.len() >= 2 {
                let r0 = scval_to_u128(&vec.0[0]).unwrap_or(0);
                let r1 = scval_to_u128(&vec.0[1]).unwrap_or(0);
                return Ok((r0, r1));
            }
        }

        // Try Map format
        if let xdr::ScVal::Map(Some(map)) = &result {
            let mut reserves = Vec::new();
            for entry in map.0.iter() {
                if let Ok(v) = scval_to_u128(&entry.val) {
                    reserves.push(v);
                }
            }
            if reserves.len() >= 2 {
                return Ok((reserves[0], reserves[1]));
            }
        }

        Err(anyhow::anyhow!("Could not parse reserves for pool {}", pool_address))
    }

    /// Parse the result of get_pools_for_tokens_range.
    /// Returns Vec<(tokens_vec, pools_map)> where each entry is a token pair with its pools.
    async fn parse_pools_result(
        &self,
        val: &xdr::ScVal,
        pools: &mut Vec<(AdapterTradingPair, PoolMeta)>,
    ) {
        let entries = match val {
            xdr::ScVal::Vec(Some(v)) => &v.0,
            _ => return,
        };

        for entry in entries.iter() {
            if let xdr::ScVal::Vec(Some(pair)) = entry {
                if pair.0.len() < 2 {
                    continue;
                }

                // Parse token addresses
                let token_addresses = match self.parse_address_vec(&pair.0[0]) {
                    Some(addrs) if addrs.len() >= 2 => addrs,
                    _ => continue,
                };

                // Skip 3+ token pools (not supported by our 2-token AMM math)
                if token_addresses.len() > 2 {
                    continue;
                }

                let token_a_addr = &token_addresses[0];
                let token_b_addr = &token_addresses[1];

                // Use contract address directly (no name() RPC call)
                let token_a = TokenId::Contract { address: token_a_addr.clone() };
                let token_b = TokenId::Contract { address: token_b_addr.clone() };

                let is_stable = is_stable_pair(&token_a, &token_b);

                // Parse pools map
                if let xdr::ScVal::Map(Some(map)) = &pair.0[1] {
                    for map_entry in map.0.iter() {
                        if let Ok(pool_address) = scval_to_address(&map_entry.val) {
                            let meta = PoolMeta {
                                is_stable,
                                fee_bps: 30, // Default, will be fetched later
                            };

                            pools.push((
                                AdapterTradingPair {
                                    token_a: token_a.clone(),
                                    token_b: token_b.clone(),
                                    pool_address,
                                    fee_bps: 30,
                                    reserve_a: None,
                                    reserve_b: None,
                                },
                                meta,
                            ));
                        }
                    }
                }
            }
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
        if addrs.is_empty() { None } else { Some(addrs) }
    }

    async fn resolve_token(&self, contract_address: &str) -> TokenId {
        match self.rpc.call_no_args(contract_address, "name").await {
            Ok(val) => {
                if let Ok(name) = scval_to_string(&val) {
                    if name == "native" {
                        return TokenId::Native;
                    }
                    if name.contains(':') {
                        return TokenId::from_str_auto(&name);
                    }
                }
            }
            Err(_) => {}
        }
        TokenId::Contract { address: contract_address.to_string() }
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
            meta_map.insert(pair.pool_address.clone(), meta);
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
        let pairs = self.pairs.read().await;
        let pair = pairs.iter().find(|p| p.pool_address == pool_address);

        let pair = match pair {
            Some(p) => p,
            None => return Ok(None),
        };

        let meta_map = self.pool_meta.read().await;
        let meta = meta_map.get(pool_address).cloned().unwrap_or(PoolMeta {
            is_stable: false,
            fee_bps: 30,
        });

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
            Self::stable_swap_quote(amount_in, reserve_in, reserve_out, meta.fee_bps)
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
        self.rpc.call_no_args(AQUARIUS_ROUTER, "get_tokens_sets_count").await.is_ok()
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        // Use batch getLedgerEntries to refresh all Aquarius pool reserves
        let pairs = self.pairs.read().await;
        if pairs.is_empty() { return Ok(0); }

        let pool_addresses: Vec<String> = pairs.iter().map(|p| p.pool_address.clone()).collect();
        drop(pairs);

        // Reuse the same batch refresh as Soroswap (same instance storage layout)
        let results = crate::batch_refresh::batch_refresh_soroswap_reserves(&self.rpc, &pool_addresses).await?;

        let mut updated = 0;
        let mut pairs = self.pairs.write().await;
        for (addr, reserves) in &results {
            if let Some((r0, r1)) = reserves {
                if let Some(pair) = pairs.iter_mut().find(|p| &p.pool_address == addr) {
                    pair.reserve_a = Some(*r0);
                    pair.reserve_b = Some(*r1);
                    updated += 1;
                }
            }
        }

        Ok(updated)
    }
}

/// Check if both tokens are stablecoins (by contract address)
fn is_stable_pair(token_a: &TokenId, token_b: &TokenId) -> bool {
    let a_str = token_a.canonical();
    let b_str = token_b.canonical();
    let a_stable = STABLE_ASSETS.contains(&a_str.as_str());
    let b_stable = STABLE_ASSETS.contains(&b_str.as_str());
    a_stable && b_stable
}

// ===== Curve math =====

fn compute_d(xp: [u128; 2], ann: u128) -> u128 {
    let sum = xp[0].saturating_add(xp[1]);
    if sum == 0 { return 0; }

    let mut d = sum;
    for _ in 0..255 {
        let d_prod = d
            .checked_mul(d)
            .and_then(|v| v.checked_mul(d))
            .and_then(|v| v.checked_div(4 * xp[0]))
            .and_then(|v| v.checked_div(xp[1]))
            .unwrap_or(u128::MAX);

        let d_prev = d;
        let numerator = ann.saturating_mul(sum).saturating_add(d_prod.saturating_mul(2)).saturating_mul(d);
        let denominator = ann.saturating_sub(1).saturating_mul(d).saturating_add(d_prod.saturating_mul(3));
        if denominator == 0 { break; }
        d = numerator / denominator;
        if d.abs_diff(d_prev) <= 1 { break; }
    }
    d
}

fn compute_y(x_new: u128, ann: u128, d: u128) -> u128 {
    let c = d
        .checked_mul(d)
        .and_then(|v| v.checked_mul(d))
        .and_then(|v| v.checked_div(ann))
        .and_then(|v| v.checked_div(4 * x_new))
        .unwrap_or(0);

    let d_over_ann = d / ann;
    let b_raw = x_new.saturating_add(d_over_ann);
    let (b_abs, b_negative) = if b_raw >= d {
        (b_raw - d, false)
    } else {
        (d - b_raw, true)
    };

    let mut y = d;
    for _ in 0..255 {
        let y_prev = y;
        let numerator = y.saturating_mul(y).saturating_add(c);
        let denominator = if b_negative {
            let two_y = 2 * y;
            if two_y > b_abs { two_y - b_abs } else { 1 }
        } else {
            2 * y + b_abs
        };
        if denominator == 0 { break; }
        y = numerator / denominator;
        if y.abs_diff(y_prev) <= 1 { break; }
    }
    y
}

//! Soroswap adapter: Uniswap V2-style AMM on Soroban.
//!
//! Key characteristics:
//! - Constant product formula: x * y = k
//! - Fee: 0.3% (30 bps) with ceiling division
//! - Factory contract provides pair discovery
//! - Each pair is a separate contract with token_0(), token_1(), get_reserves()

use crate::rpc::{scval_to_address, scval_to_i128, scval_to_u128, scval_to_u32, SorobanRpc};
use crate::traits::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Soroswap Factory contract address (Mainnet)
pub const SOROSWAP_FACTORY: &str = "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2";

/// Batch concurrency for RPC calls
const BATCH_SIZE: usize = 20;
/// Delay between batches (ms)
const BATCH_DELAY_MS: u64 = 100;

pub struct SoroswapAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
}

impl SoroswapAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
        }
    }

    /// Batch-refresh all pool reserves using a single getLedgerEntries call.
    /// This is ~200x faster than calling get_reserves() on each pool individually.
    pub async fn refresh_all_reserves(&self) -> Result<usize> {
        let pairs = self.pairs.read().await;
        if pairs.is_empty() {
            return Ok(0);
        }

        let pool_addresses: Vec<String> = pairs.iter().map(|p| p.pool_address.clone()).collect();
        drop(pairs); // Release read lock before write

        let concurrency = std::env::var("POOL_STATE_REFRESH_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        let results = crate::batch_refresh::batch_refresh_soroswap_reserves_parallel(
            &self.rpc,
            &pool_addresses,
            concurrency,
        )
        .await?;

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

        debug!(
            "Soroswap: batch-refreshed {}/{} pools",
            updated,
            pool_addresses.len()
        );
        Ok(updated)
    }

    /// Compute output amount using constant product formula.
    /// Matches on-chain Soroswap logic exactly:
    ///   fee = ceil(amount_in * 3 / 1000)
    ///   in_after_fee = amount_in - fee
    ///   amount_out = in_after_fee * reserve_out / (reserve_in + in_after_fee)
    pub fn compute_output(amount_in: u128, reserve_in: u128, reserve_out: u128) -> u128 {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return 0;
        }
        let fee = (amount_in * 3 + 999) / 1000; // ceiling division
        let in_after_fee = amount_in - fee;
        let numerator = in_after_fee * reserve_out;
        let denominator = reserve_in + in_after_fee;
        numerator / denominator
    }

    fn compute_price_impact(amount_in: u128, reserve_in: u128) -> u32 {
        if reserve_in == 0 {
            return 10_000;
        }
        let impact = amount_in * 10_000 / (2 * reserve_in);
        impact.min(10_000) as u32
    }

    /// Fetch all pairs from the Soroswap Factory contract.
    /// Optimized: uses contract addresses directly as TokenId (no name() calls).
    async fn fetch_pairs_from_factory(&self) -> Result<Vec<AdapterTradingPair>> {
        // 1. Get total pair count
        let length_val = self
            .rpc
            .call_no_args(SOROSWAP_FACTORY, "all_pairs_length")
            .await?;
        let total_pairs = scval_to_u32(&length_val)?;
        info!("Soroswap: total pairs = {}", total_pairs);

        if total_pairs == 0 {
            return Ok(vec![]);
        }

        // 2. Fetch pair addresses in large batches
        let mut all_pairs = Vec::new();
        let indices: Vec<u32> = (0..total_pairs).collect();

        for chunk in indices.chunks(BATCH_SIZE) {
            let futures: Vec<_> = chunk.iter().map(|&i| self.fetch_pair_fast(i)).collect();
            let results = futures::future::join_all(futures).await;

            for result in results {
                match result {
                    Ok(Some(pair)) => all_pairs.push(pair),
                    Ok(None) => {}
                    Err(e) => debug!("Soroswap pair {} fetch error: {}", chunk[0], e),
                }
            }

            if all_pairs.len() % 50 == 0 && !all_pairs.is_empty() {
                info!(
                    "Soroswap: fetched {}/{} pairs so far",
                    all_pairs.len(),
                    total_pairs
                );
            }

            // Small delay to avoid overwhelming RPC
            tokio::time::sleep(std::time::Duration::from_millis(BATCH_DELAY_MS)).await;
        }

        info!("Soroswap: fetched {} valid pairs total", all_pairs.len());
        Ok(all_pairs)
    }

    /// Fast pair fetch: get address + tokens + reserves in minimal RPC calls.
    /// Uses contract address as TokenId directly (no name() resolution).
    async fn fetch_pair_fast(&self, index: u32) -> Result<Option<AdapterTradingPair>> {
        let index_val = xdr::ScVal::U32(index);

        // Get pair contract address
        let pair_addr_val = self
            .rpc
            .simulate_call(SOROSWAP_FACTORY, "all_pairs", vec![index_val])
            .await?;
        let pair_address = scval_to_address(&pair_addr_val)?;

        // Get token addresses + reserves in parallel
        let (token_0_result, token_1_result, reserves_result) = tokio::join!(
            self.rpc.call_no_args(&pair_address, "token_0"),
            self.rpc.call_no_args(&pair_address, "token_1"),
            self.rpc.call_no_args(&pair_address, "get_reserves"),
        );

        let token_a_addr = scval_to_address(&token_0_result?)?;
        let token_b_addr = scval_to_address(&token_1_result?)?;

        // Use contract address directly as TokenId (fast, no extra RPC)
        let token_a = TokenId::Contract {
            address: token_a_addr,
        };
        let token_b = TokenId::Contract {
            address: token_b_addr,
        };

        // Parse reserves
        let (reserve_a, reserve_b) = match reserves_result {
            Ok(val) => parse_reserves(&val).unwrap_or((None, None)),
            Err(_) => (None, None),
        };

        // Skip pools with zero liquidity (but keep pools where reserves couldn't be read)
        if reserve_a == Some(0) && reserve_b == Some(0) {
            return Ok(None);
        }

        Ok(Some(AdapterTradingPair {
            token_a,
            token_b,
            pool_address: pair_address,
            fee_bps: 30,
            reserve_a,
            reserve_b,
        }))
    }
}

/// Parse reserves from get_reserves() return value.
fn parse_reserves(val: &xdr::ScVal) -> Result<(Option<u128>, Option<u128>)> {
    if let xdr::ScVal::Vec(Some(vec)) = val {
        if vec.0.len() >= 2 {
            let r0 = scval_to_i128(&vec.0[0])
                .map(|v| v as u128)
                .or_else(|_| scval_to_u128(&vec.0[0]))?;
            let r1 = scval_to_i128(&vec.0[1])
                .map(|v| v as u128)
                .or_else(|_| scval_to_u128(&vec.0[1]))?;
            return Ok((Some(r0), Some(r1)));
        }
    }
    Ok((None, None))
}

#[async_trait]
impl DexAdapter for SoroswapAdapter {
    fn id(&self) -> &str {
        "soroswap"
    }

    fn name(&self) -> &str {
        "Soroswap"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanAmm
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let pairs = self.fetch_pairs_from_factory().await?;
        *self.pairs.write().await = pairs.clone();
        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        _token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
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

        let amount_out = Self::compute_output(amount_in, reserve_in, reserve_out);
        if amount_out == 0 {
            return Ok(None);
        }

        let price_impact_bps = Self::compute_price_impact(amount_in, reserve_in);

        Ok(Some(AdapterQuote {
            amount_out,
            fee_bps: 30,
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
            .call_no_args(SOROSWAP_FACTORY, "all_pairs_length")
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
    fn test_compute_output_basic() {
        let out = SoroswapAdapter::compute_output(10_0000000, 100_000_0000000, 100_000_0000000);
        assert!(out > 9_9000000 && out < 10_0000000);
    }

    #[test]
    fn test_compute_output_zero_reserves() {
        assert_eq!(SoroswapAdapter::compute_output(1000, 0, 100_000), 0);
        assert_eq!(SoroswapAdapter::compute_output(1000, 100_000, 0), 0);
    }

    #[test]
    fn test_compute_output_zero_input() {
        assert_eq!(SoroswapAdapter::compute_output(0, 100_000, 100_000), 0);
    }

    #[test]
    fn test_fee_ceiling_division() {
        let out = SoroswapAdapter::compute_output(1, 1_000_000, 1_000_000);
        assert_eq!(out, 0);

        let out = SoroswapAdapter::compute_output(334, 1_000_000, 1_000_000);
        assert!(out > 0);
    }

    #[test]
    fn test_large_trade_price_impact() {
        let reserve = 1_000_000_0000000u128;
        let amount_in = 100_000_0000000u128;
        let out = SoroswapAdapter::compute_output(amount_in, reserve, reserve);
        assert!(out < 100_000_0000000);
        assert!(out > 90_000_0000000);
    }
}

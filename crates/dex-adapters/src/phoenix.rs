//! Phoenix adapter: Soroban AMM with fee-on-output (commission model).
//!
//! Key characteristics:
//! - Factory contract: CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI
//! - Fee applied on OUTPUT (commission), not input
//! - Formula: gross_return = offer_amount * ask_pool / (offer_pool +
//!   offer_amount) commission = gross_return * fee_bps / 10_000 net_return =
//!   gross_return - commission
//! - Pool discovery via query_all_pools_details()

use {
    crate::{
        rpc::{get_map_field, scval_to_address, scval_to_i128, SorobanRpc},
        traits::*,
    },
    anyhow::Result,
    async_trait::async_trait,
    std::{collections::HashMap, sync::Arc},
    stellar_xdr::curr as xdr,
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

/// Phoenix Factory contract address (Mainnet)
pub const PHOENIX_FACTORY: &str = "CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI";

pub struct PhoenixAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
    pool_fees: RwLock<HashMap<String, u32>>,
}

impl PhoenixAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
            pool_fees: RwLock::new(HashMap::new()),
        }
    }

    /// Phoenix quote: fee on output.
    /// gross_return = amount_in * reserve_out / (reserve_in + amount_in)
    /// commission = gross_return * fee_bps / 10_000
    /// net_return = gross_return - commission
    pub fn compute_output(amount_in: u128, reserve_in: u128, reserve_out: u128, fee_bps: u32) -> u128 {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return 0;
        }
        let gross_return = amount_in * reserve_out / (reserve_in + amount_in);
        let commission = gross_return * fee_bps as u128 / 10_000;
        gross_return - commission
    }

    /// Fetch all pools from Phoenix Factory via query_all_pools_details().
    async fn fetch_pools_from_factory(&self) -> Result<Vec<(AdapterTradingPair, u32)>> {
        let result = self
            .rpc
            .call_no_args(PHOENIX_FACTORY, "query_all_pools_details")
            .await?;

        let entries = match &result {
            xdr::ScVal::Vec(Some(v)) => &v.0,
            _ => {
                warn!("Phoenix factory returned unexpected type");
                return Ok(vec![]);
            }
        };

        let mut pools = Vec::new();

        for entry in entries.iter() {
            match self.parse_pool_info(entry).await {
                Ok(Some(pool)) => pools.push(pool),
                Ok(None) => {}
                Err(e) => debug!("Phoenix pool parse error: {}", e),
            }
        }

        info!("Phoenix: fetched {} pools", pools.len());
        Ok(pools)
    }

    /// Parse a single LiquidityPoolInfo entry.
    /// Structure: { pool_address, pool_response: { asset_a: {address, amount},
    /// asset_b: {address, amount} }, total_fee_bps }
    async fn parse_pool_info(&self, val: &xdr::ScVal) -> Result<Option<(AdapterTradingPair, u32)>> {
        let map = match val {
            xdr::ScVal::Map(Some(m)) => m,
            _ => return Ok(None),
        };

        // pool_address
        let pool_address = get_map_field(map, "pool_address")
            .ok_or_else(|| anyhow::anyhow!("missing pool_address"))
            .and_then(scval_to_address)?;

        // total_fee_bps
        let fee_bps = get_map_field(map, "total_fee_bps")
            .and_then(|v| scval_to_i128(v).ok())
            .unwrap_or(30) as u32;

        // pool_response -> asset_a, asset_b
        let pool_response = match get_map_field(map, "pool_response") {
            Some(xdr::ScVal::Map(Some(m))) => m,
            _ => return Ok(None),
        };

        let (token_a_addr, reserve_a) = self.parse_asset_field(pool_response, "asset_a")?;
        let (token_b_addr, reserve_b) = self.parse_asset_field(pool_response, "asset_b")?;

        let token_a = Self::token_from_contract_address(&token_a_addr);
        let token_b = Self::token_from_contract_address(&token_b_addr);

        Ok(Some((
            AdapterTradingPair {
                token_a,
                token_b,
                pool_address,
                fee_bps,
                reserve_a: Some(reserve_a as u128),
                reserve_b: Some(reserve_b as u128),
            },
            fee_bps,
        )))
    }

    /// Parse an Asset field: { address: Address, amount: i128 }
    fn parse_asset_field(&self, map: &xdr::ScMap, key: &str) -> Result<(String, i128)> {
        let asset_val = get_map_field(map, key).ok_or_else(|| anyhow::anyhow!("missing {}", key))?;

        let asset_map = match asset_val {
            xdr::ScVal::Map(Some(m)) => m,
            _ => anyhow::bail!("expected Map for {}", key),
        };

        let address = get_map_field(asset_map, "address")
            .ok_or_else(|| anyhow::anyhow!("missing address in {}", key))
            .and_then(scval_to_address)?;

        let amount = get_map_field(asset_map, "amount")
            .and_then(|v| scval_to_i128(v).ok())
            .unwrap_or(0);

        Ok((address, amount))
    }

    /// Refresh reserves for specific pools (one factory RPC, patch cached
    /// pairs).
    pub async fn refresh_touched_pools(&self, pool_addresses: &[String]) -> Result<usize> {
        if pool_addresses.is_empty() {
            return Ok(0);
        }
        let wanted: std::collections::HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
        let results = self.fetch_pools_from_factory().await?;
        let mut pairs = self.pairs.write().await;
        let mut fees = self.pool_fees.write().await;
        let mut updated = 0usize;
        for (pair, fee_bps) in results {
            if !wanted.contains(pair.pool_address.as_str()) {
                continue;
            }
            if let Some(existing) = pairs.iter_mut().find(|p| p.pool_address == pair.pool_address) {
                let pool_address = existing.pool_address.clone();
                *existing = pair;
                fees.insert(pool_address, fee_bps);
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// SAC contract address — same identity as Aquarius/Soroswap graph edges.
    fn token_from_contract_address(contract_address: &str) -> TokenId {
        TokenId::Contract {
            address: contract_address.to_string(),
        }
    }
}

#[async_trait]
impl DexAdapter for PhoenixAdapter {
    fn id(&self) -> &str {
        "phoenix"
    }

    fn name(&self) -> &str {
        "Phoenix"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanAmm
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let results = self.fetch_pools_from_factory().await?;

        let mut pairs = Vec::new();
        let mut fees = HashMap::new();

        for (pair, fee_bps) in results {
            fees.insert(pair.pool_address.clone(), fee_bps);
            pairs.push(pair);
        }

        *self.pairs.write().await = pairs.clone();
        *self.pool_fees.write().await = fees;

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

        let fees = self.pool_fees.read().await;
        let fee_bps = fees.get(pool_address).copied().unwrap_or(30);

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

        let amount_out = Self::compute_output(amount_in, reserve_in, reserve_out, fee_bps);
        if amount_out == 0 {
            return Ok(None);
        }

        let price_impact_bps = (amount_in * 10_000 / (2 * reserve_in)).min(10_000) as u32;

        Ok(Some(AdapterQuote {
            amount_out,
            fee_bps,
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
            .call_no_args(PHOENIX_FACTORY, "query_all_pools_details")
            .await
            .is_ok()
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        let results = self.fetch_pools_from_factory().await?;
        if results.is_empty() {
            return Ok(0);
        }
        let mut pairs = Vec::new();
        let mut fees = HashMap::new();
        for (pair, fee_bps) in results {
            fees.insert(pair.pool_address.clone(), fee_bps);
            pairs.push(pair);
        }
        let updated = pairs.len();
        *self.pairs.write().await = pairs;
        *self.pool_fees.write().await = fees;
        Ok(updated)
    }

    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        self.pairs.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phoenix_output() {
        // 100 in, reserves 10000/10000, fee 30 bps
        let out = PhoenixAdapter::compute_output(100, 10_000, 10_000, 30);
        // gross = 100 * 10000 / (10000 + 100) ≈ 99
        // commission = 99 * 30 / 10000 ≈ 0
        // net ≈ 99
        assert!(out > 95 && out < 100);
    }

    #[test]
    fn test_phoenix_high_fee() {
        let out = PhoenixAdapter::compute_output(1000, 100_000, 100_000, 300); // 3% fee
                                                                               // gross ≈ 990, commission ≈ 29.7, net ≈ 960
        assert!(out > 950 && out < 980);
    }

    #[test]
    fn test_phoenix_zero() {
        assert_eq!(PhoenixAdapter::compute_output(0, 100_000, 100_000, 30), 0);
        assert_eq!(PhoenixAdapter::compute_output(100, 0, 100_000, 30), 0);
        assert_eq!(PhoenixAdapter::compute_output(100, 100_000, 0, 30), 0);
    }
}

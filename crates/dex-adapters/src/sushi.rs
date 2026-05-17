//! Sushi adapter: V3 Concentrated Liquidity AMM on Soroban (deployed by SushiSwap team).
//!
//! Sushi went live on Stellar mainnet in late 2025.
//! TVL: ~$1.95M (per DeFiLlama).
//!
//! This is a V3 (CLMM) deployment with tick-based pricing.
//! Contracts:
//!   Router: CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL
//!   Factory: CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF
//!
//! Quote approach: use simulateTransaction on the Router's swap_exact_input_hints
//! to get quotes. The contract computes the exact output including tick traversal.
//! This is a "black-box" approach — we don't need to understand tick data locally.

use crate::rpc::{SorobanRpc, scval_to_address, scval_to_i128, scval_to_u128};
use crate::traits::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::sync::Arc;
use stellar_xdr::curr as xdr;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// Sushi V3 Router contract address on Stellar Mainnet
pub const SUSHI_ROUTER: &str = "CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL";

/// Sushi V3 Factory contract address on Stellar Mainnet
pub const SUSHI_FACTORY: &str = "CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF";

/// Known Sushi V3 fee tiers (basis points)
const FEE_TIERS: &[u32] = &[500, 3000, 10000]; // 0.05%, 0.3%, 1%

pub struct SushiAdapter {
    rpc: Arc<SorobanRpc>,
    /// Discovered pools: (token_a, token_b, pool_address, fee_bps)
    pairs: RwLock<Vec<AdapterTradingPair>>,
}

impl SushiAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
        }
    }

    /// Get a quote by simulating the Router's swap_exact_input_hints function.
    /// This is the most accurate way to quote V3 — the contract handles tick traversal.
    async fn simulate_quote(
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

        // Build args for swap_exact_input_hints:
        // swap_exact_input_hints(amount_in, amount_out_minimum, deadline, path, fees, recipient, sender, checkpoints)
        let amount_in_val = xdr::ScVal::I128(xdr::Int128Parts {
            hi: (amount_in as i128 >> 64) as i64,
            lo: amount_in as u64,
        });

        // amount_out_minimum = 0 (we just want the quote, not enforcing minimum)
        let min_out_val = xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 0 });

        // deadline = far future
        let deadline_val = xdr::ScVal::U64(1779031769);

        // path = [token_in, token_out]
        let path_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
        ].try_into().unwrap())));

        // fees = [fee_bps]
        let fees_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![
            xdr::ScVal::U32(fee_bps),
        ].try_into().unwrap())));

        // recipient = dummy address (simulation only)
        let dummy_addr = xdr::ScVal::Address(xdr::ScAddress::Account(
            xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([0u8; 32])))
        ));

        // checkpoints = empty vec
        let checkpoints_val = xdr::ScVal::Vec(Some(xdr::ScVec(vec![].try_into().unwrap())));

        let args = vec![
            amount_in_val,
            min_out_val,
            deadline_val,
            path_val,
            fees_val,
            dummy_addr.clone(), // recipient
            dummy_addr,         // sender
            checkpoints_val,
        ];

        match self.rpc.simulate_call(SUSHI_ROUTER, "swap_exact_input_hints", args).await {
            Ok(result) => {
                // Result should be the output amount (i128 or u128)
                if let Ok(amount_out) = scval_to_i128(&result) {
                    return Ok(Some(amount_out as u128));
                }
                if let Ok(amount_out) = scval_to_u128(&result) {
                    return Ok(Some(amount_out));
                }
                debug!("Sushi quote returned unexpected ScVal: {:?}", std::mem::discriminant(&result));
                Ok(None)
            }
            Err(e) => {
                debug!("Sushi simulate quote failed: {}", e);
                Ok(None)
            }
        }
    }

    /// Discover pools by querying the factory for known token pairs.
    async fn discover_pools(&self) -> Result<Vec<AdapterTradingPair>> {
        // Well-known tokens to check for Sushi pools
        let tokens = vec![
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA", // XLM
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", // EURC
        ];

        let mut pools = Vec::new();

        // Try each pair + fee tier combination
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
                                reserve_a: None, // V3 doesn't have simple reserves
                                reserve_b: None,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => debug!("Sushi pool discovery error: {}", e),
                    }
                }
            }
        }

        info!("Sushi: discovered {} pools", pools.len());
        Ok(pools)
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
                    Ok(addr) if !addr.is_empty() => Ok(Some(addr)),
                    _ => Ok(None),
                }
            }
            Err(_) => Ok(None),
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
        ProtocolType::SorobanAmm // Could add a new SorobanClmm variant
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let pairs = self.discover_pools().await?;
        *self.pairs.write().await = pairs.clone();
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
        let pair = match pairs.iter().find(|p| p.pool_address == pool_address) {
            Some(p) => p,
            None => return Ok(None),
        };

        let token_in_addr = token_in.canonical();
        let token_out_addr = token_out.canonical();

        // Use simulate to get exact quote from the V3 router
        match self.simulate_quote(&token_in_addr, &token_out_addr, amount_in, pair.fee_bps).await? {
            Some(amount_out) if amount_out > 0 => {
                Ok(Some(AdapterQuote {
                    amount_out,
                    fee_bps: pair.fee_bps,
                    price_impact_bps: 0, // V3 impact is complex, skip for now
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
        // Try to call get_pool on factory
        self.rpc.call_no_args(SUSHI_FACTORY, "get_protocol_fee_0").await.is_ok()
    }
}

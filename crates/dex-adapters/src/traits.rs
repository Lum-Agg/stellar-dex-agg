//! Core trait that all DEX adapters must implement.

use {
    anyhow::Result,
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
};

/// Token identifier (mirrors router-engine's TokenId but avoids circular dep).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenId {
    Native,
    Classic { code: String, issuer: String },
    Contract { address: String },
}

impl TokenId {
    pub fn from_str_auto(s: &str) -> Self {
        if s == "native" {
            return Self::Native;
        }
        if let Some((code, issuer)) = s.split_once(':') {
            return Self::Classic {
                code: code.to_string(),
                issuer: issuer.to_string(),
            };
        }
        Self::Contract { address: s.to_string() }
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Native => "native".to_string(),
            Self::Classic { code, issuer } => format!("{}:{}", code, issuer),
            Self::Contract { address } => address.clone(),
        }
    }
}

impl std::fmt::Display for TokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.canonical())
    }
}

/// Protocol type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolType {
    /// Soroban-based AMM (Uniswap V2 style)
    SorobanAmm,
    /// Soroban-based weighted pool (Balancer style)
    SorobanWeightedPool,
    /// Soroban-based stable swap (Curve style)
    SorobanStableSwap,
    /// Stellar native DEX (orderbook + liquidity pools)
    ClassicDex,
}

/// A trading pair available on a DEX.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterTradingPair {
    pub token_a: TokenId,
    pub token_b: TokenId,
    pub pool_address: String,
    pub fee_bps: u32,
    pub reserve_a: Option<u128>,
    pub reserve_b: Option<u128>,
}

/// Quote result from a single DEX hop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterQuote {
    pub amount_out: u128,
    pub fee_bps: u32,
    pub price_impact_bps: u32,
}

/// Swap operation parameters for transaction building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwapOperation {
    /// Soroban contract invocation
    SorobanInvoke {
        contract_id: String,
        function_name: String,
        /// XDR-encoded arguments (base64)
        args_xdr: Vec<String>,
    },
    /// Classic DEX PathPaymentStrictSend
    ClassicPathPayment {
        send_asset: String,
        dest_asset: String,
        send_amount: i64,
        dest_min: i64,
        /// Intermediate assets in the path
        path: Vec<String>,
    },
}

/// The core trait all DEX adapters must implement.
#[async_trait]
pub trait DexAdapter: Send + Sync {
    /// Unique identifier for this adapter (e.g., "soroswap", "aquarius").
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Protocol type classification.
    fn protocol_type(&self) -> ProtocolType;

    /// Fetch all available trading pairs from this DEX.
    /// This may involve RPC calls to factory contracts or API queries.
    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>>;

    /// Get a quote for swapping `amount_in` of `token_in` to `token_out`
    /// through the specified pool.
    ///
    /// Returns None if the pair is not available or liquidity is insufficient.
    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>>;

    /// Build the swap operation parameters for transaction construction.
    async fn build_swap_op(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        min_amount_out: u128,
        pool_address: &str,
    ) -> Result<SwapOperation>;

    /// Health check: verify the adapter can reach its data source.
    async fn health_check(&self) -> bool;

    /// Fast batch refresh of pool reserves (if supported).
    /// Returns the number of pools updated.
    async fn refresh_reserves(&self) -> Result<usize> {
        Ok(0)
    }

    /// Get the currently cached pairs without re-fetching from chain.
    /// Used after refresh_reserves() to update the quote engine's local cache.
    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        vec![]
    }
}

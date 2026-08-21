// Use TokenId from dex-adapters as the single source of truth
pub use dex_adapters::TokenId;
use serde::{Deserialize, Serialize};

/// A trading pair on a specific DEX source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub token_a: TokenId,
    pub token_b: TokenId,
    pub source: String,
    pub pool_address: String,
    pub fee_bps: u32,
    pub reserve_a: Option<u128>,
    pub reserve_b: Option<u128>,
}

/// Standardized quote from a single DEX source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub source: String,
    pub pool_address: String,
    pub token_in: TokenId,
    pub token_out: TokenId,
    pub amount_in: u128,
    pub amount_out: u128,
    /// Price impact in basis points (e.g., 50 = 0.5%)
    pub price_impact_bps: u32,
    pub fee_bps: u32,
    /// Intermediate tokens in the path (empty for direct swaps)
    pub path: Vec<TokenId>,
    pub timestamp_ms: u64,
}

/// A discovered path from token_in to token_out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    /// Sequence of tokens: [token_in, intermediate_1, ..., token_out]
    pub tokens: Vec<TokenId>,
    /// DEX source for each hop (len = tokens.len() - 1)
    pub sources: Vec<String>,
    /// Pool address for each hop
    pub pool_addresses: Vec<String>,
    /// Number of hops
    pub hops: usize,
}

/// A sub-order in a split execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubOrder {
    pub path: Path,
    pub amount_in: u128,
    pub expected_amount_out: u128,
    /// Fraction of total input allocated to this sub-order (0.0 - 1.0)
    pub fraction: f64,
}

/// The optimal route computed by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDebugCandidate {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub amount_out: u128,
    pub price_impact_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDebugPlannedSplit {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub amount_in: u128,
    pub expected_amount_out: u128,
    pub fraction_bps: u32,
}

/// The optimal route computed by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDebug {
    pub quoted_paths_count: usize,
    pub candidate_paths_count: usize,
    pub best_single_out: u128,
    pub second_best_out: Option<u128>,
    pub best_single_impact_bps: u32,
    pub split_threshold_bps: u32,
    pub competitive_delta_bps: u32,
    pub min_split_fraction_bps: u32,
    pub split_attempted: bool,
    pub split_rejected_reason: Option<String>,
    pub optimization_strategy: String,
    pub used_rest_best_approximation: bool,
    pub split_total_out: Option<u128>,
    pub dust_filtered_legs: usize,
    pub candidate_routes: Vec<RouteDebugCandidate>,
    pub planned_split: Vec<RouteDebugPlannedSplit>,
}

/// The optimal route computed by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimalRoute {
    pub sub_orders: Vec<SubOrder>,
    pub total_amount_in: u128,
    pub total_expected_out: u128,
    /// Aggregate price impact in basis points
    pub price_impact_bps: u32,
    pub is_split: bool,
    /// Improvement over best single path (basis points)
    pub improvement_bps: u32,
    /// Minimum output after slippage (set by caller)
    pub minimum_out: u128,
    /// Computation time in milliseconds
    pub compute_time_ms: u64,
    /// Optional debug metadata explaining routing/split decisions.
    pub debug: Option<RouteDebug>,
}

/// Request to find the optimal route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub token_in: TokenId,
    pub token_out: TokenId,
    pub amount_in: u128,
    /// Slippage tolerance in basis points (default: 50 = 0.5%)
    pub slippage_bps: Option<u32>,
    /// Maximum number of hops (default: 4)
    pub max_hops: Option<usize>,
    /// Maximum number of splits (default: 5)
    pub max_splits: Option<usize>,
    /// When true, Soroban AMMs only (no Classic SDEX / no Horizon).
    /// Default may still return a pure classic route; mixed hops are never
    /// returned.
    pub prefer_soroban: Option<bool>,
}

/// Result of transaction simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub actual_output: Option<u128>,
    pub resource_fee: Option<u64>,
    pub error: Option<String>,
}

/// Unsigned transaction ready for user signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    /// XDR-encoded transaction envelope (base64)
    pub xdr: String,
    /// Transaction hash
    pub hash: String,
    /// Number of operations
    pub operation_count: u32,
    /// Estimated network fee (stroops)
    pub estimated_fee: u64,
    /// Simulation result
    pub simulation: SimulationResult,
}

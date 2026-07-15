#![no_std]

use soroban_sdk::{contracttype, Address, Vec};

/// Supported DEX protocol types (shared by aggregator + vault).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DexType {
    Aquarius,
    SoroswapPair,
    Phoenix,
    Sushi,
    CometDex,
}

/// A single swap step in the aggregation path.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SwapStep {
    pub dex_id: Address,
    pub dex_type: DexType,
    pub token_in: Address,
    pub token_out: Address,
    pub in_idx: u32,
    pub out_idx: u32,
}

/// A sub-route in a split order.
///
/// Semantics of `amount_in` depend on the call site:
/// - `swap` / `leg_out` of `round_trip_swap`: absolute token input.
/// - `leg_back` of `round_trip_swap`: positive **weight**; the contract
///   rescales weights to the actual bridge output after `leg_out` (last
///   sub-route receives the remainder).
#[contracttype]
#[derive(Clone, Debug)]
pub struct SubRoute {
    pub amount_in: i128,
    pub steps: Vec<SwapStep>,
}

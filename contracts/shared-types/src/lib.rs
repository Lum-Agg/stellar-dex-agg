#![no_std]

use soroban_sdk::{contracttype, Address, Vec};

/// Minimum output in atomic units, rounded up so the limit is never weakened.
pub fn required_min_out(amount: i128, rate_e7: i128) -> Option<i128> {
    const SCALE: i128 = 10_000_000;
    if amount <= 0 || rate_e7 <= 0 {
        return None;
    }
    // Split both factors to avoid overflow in the unscaled product.
    let remainder = amount % SCALE;
    let fractional = remainder * (rate_e7 % SCALE);
    (amount / SCALE)
        .checked_mul(rate_e7)?
        .checked_add(remainder * (rate_e7 / SCALE))?
        .checked_add(fractional / SCALE + i128::from(fractional % SCALE != 0))
}

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

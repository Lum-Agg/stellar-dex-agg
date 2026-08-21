#![no_std]
//! Stellar DEX Aggregator Contract
//!
//! Executes multi-hop and split-order swaps atomically across Soroban DEXes
//! (Aquarius, Soroswap, Phoenix, Sushi V3, Comet).
//!
//! Main entry point:
//! - `swap()`: Atomic swap via `sub_routes` (one leg = simple path; multiple =
//!   split)
//!
//! Key design: the contract holds no funds permanently.
//! Users approve token transfers, the contract executes swaps, and outputs go
//! directly back to the user — all in one atomic invocation.

mod admin;
mod auth;
mod events;
mod invoke;
mod math;
mod round_trip;
mod storage;
mod swap;
mod validate;

#[cfg(test)]
mod tests;

pub use lumagg_contract_types::{DexType, SubRoute, SwapStep};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

#[contract]
pub struct AggregatorContract;

#[contractimpl]
impl AggregatorContract {
    /// Initialize the contract with an admin address.
    /// Must be called once after deployment.
    pub fn initialize(env: Env, admin: Address) {
        admin::initialize(env, admin);
    }

    /// Upgrade the contract WASM code. Only admin can call.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        admin::upgrade(env, new_wasm_hash);
    }

    /// Get the admin address.
    pub fn admin(env: Env) -> Address {
        admin::admin(env)
    }

    /// Register or remove a DEX venue for escrow-backed fills. Public swaps
    /// remain open-routed; this registry only protects order escrow calls.
    pub fn set_venue(env: Env, dex_type: DexType, dex_id: Address, allowed: bool) {
        admin::set_venue(env, dex_type, dex_id, allowed);
    }

    pub fn is_venue(env: Env, dex_type: DexType, dex_id: Address) -> bool {
        admin::is_venue(env, dex_type, dex_id)
    }

    /// Execute a swap atomically (single-path or split-order).
    ///
    /// `sub_routes` is always a list of legs; a simple swap is one entry with
    /// the full `amount_in` and its hop `steps`. Split execution uses
    /// multiple entries.
    ///
    /// Flow:
    /// 1. Pull total input from user (sum of sub-route amounts)
    /// 2. For each sub-route: execute its path with its allocated amount
    /// 3. Sum outputs (all must produce the same `token_out`)
    /// 4. Verify total output >= `min_amount_out`
    /// 5. Transfer total output to user
    pub fn swap(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        swap::swap(env, user, token_in, token_out, sub_routes, min_amount_out)
    }

    /// Execute an escrow-backed swap using only administrator-registered
    /// venues. The caller must still authorize the escrow as `user`.
    pub fn swap_restricted(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        validate::validate_registered_venues(&env, &sub_routes);
        swap::swap(env, user, token_in, token_out, sub_routes, min_amount_out)
    }

    /// Round-trip swap: base → bridge (split OK) → base (split OK) in one
    /// atomic invocation.
    ///
    /// Funds are pulled from `user` and the final `base_token` balance is
    /// returned to `user`. The contract does not retain funds after
    /// execution.
    ///
    /// # Parameters
    ///
    /// - `leg_out`: sub-routes from `base_token` to `bridge_token`. Each
    ///   `SubRoute.amount_in` is an absolute base-token input; they **must**
    ///   sum to `amount_in`.
    /// - `leg_back`: sub-routes from `bridge_token` to `base_token`. Each
    ///   `SubRoute.amount_in` is a **positive weight** (quoted bridge amounts
    ///   are fine). After `leg_out` produces actual bridge total `o1`, weights
    ///   are rescaled so executed inputs sum **exactly** to `o1` (last
    ///   sub-route receives the remainder). Callers do **not** need to know
    ///   `o1` at submit time.
    /// - `min_amount_out`: minimum total `base_token` returned (principal +
    ///   profit floor)
    ///
    /// # Integrator note
    ///
    /// Same `SubRoute` type for both legs — no extra fields. Semantics of
    /// `amount_in` differ by leg: absolute on `leg_out`, proportional weight
    /// on `leg_back`.
    pub fn round_trip_swap(
        env: Env,
        user: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        round_trip::round_trip_swap(
            env,
            user,
            base_token,
            bridge_token,
            amount_in,
            leg_out,
            leg_back,
            min_amount_out,
        )
    }
}

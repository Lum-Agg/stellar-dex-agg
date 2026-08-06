#![no_std]
//! LumAgg arb vault: holds trading float; authorized callers execute round-trip
//! swaps via the aggregator without pre-funding bot wallets with principal.
//!
//! Flow inside `execute_round_trip`:
//! 1. Caller approves vault for a fixed ceiling (not the simulated return)
//! 2. Transfer `amount_in` base token from vault → caller
//! 3. Cross-call `aggregator.round_trip_swap(user = caller, ...)`
//! 4. `transfer_from` reclaim of actual `base_total` (no exact-amount pre-sign)
//!
//! # Soroban auth pitfall (do not regress)
//!
//! Any value that ends up in a nested `require_auth` call (SAC `approve`,
//! `transfer`, …) is **pinned into the signed auth tree at simulate time**.
//! If the contract recomputes that value from `env.ledger().sequence()` (or
//! from a simulated return amount), inclusion a ledger or two later uses
//! different args → auth miss.
//!
//! Symptom on mainnet (2026-07-16, after `approve(MAX, sequence()+20)`):
//! `Unauthorized function call for address` on the caller G-address at the
//! nested `approve`, while simulate still succeeds.
//!
//! Rules:
//! - Reclaim **amount**: use `i128::MAX` approve + `transfer_from(actual)` —
//!   never pin `base_total` from sim into auth.
//! - Approve **expiration**: pass as a call argument from the bot
//!   (`allowance_expiration_ledger`). Never `sequence()+N` inside this
//!   contract. Do not use `u32::MAX` either (SAC rejects past max TTL).

mod admin;
mod auth;
mod execute;
mod storage;
mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};
use lumagg_contract_types::SubRoute;

pub use types::AggregatorContract;

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn initialize(env: Env, admin: Address) {
        admin::initialize(env, admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        admin::upgrade(env, new_wasm_hash);
    }

    pub fn admin(env: Env) -> Address {
        admin::admin(env)
    }

    pub fn add_caller(env: Env, caller: Address) {
        admin::add_caller(env, caller);
    }

    pub fn remove_caller(env: Env, caller: Address) {
        admin::remove_caller(env, caller);
    }

    pub fn is_caller(env: Env, caller: Address) -> bool {
        admin::is_caller(env, caller)
    }

    /// Pull tokens from `from` into the vault (any account may fund the vault).
    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
        execute::deposit(env, from, token, amount);
    }

    /// Admin emergency withdrawal from vault balances.
    pub fn admin_withdraw(env: Env, token: Address, to: Address, amount: i128) {
        admin::admin_withdraw(env, token, to, amount);
    }

    /// Authorized caller executes a round-trip arb atomically:
    /// vault → caller → aggregator → caller → vault.
    ///
    /// `allowance_expiration_ledger` — client-chosen SAC approve expiry (see
    /// crate-level "Soroban auth pitfall"). Must be ≥ current ledger and
    /// within network max entry TTL; bot typically sends `latest + ~100k`.
    pub fn execute_round_trip(
        env: Env,
        caller: Address,
        aggregator: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
        allowance_expiration_ledger: u32,
    ) -> i128 {
        execute::execute_round_trip(
            env,
            caller,
            aggregator,
            base_token,
            bridge_token,
            amount_in,
            leg_out,
            leg_back,
            min_amount_out,
            allowance_expiration_ledger,
        )
    }
}

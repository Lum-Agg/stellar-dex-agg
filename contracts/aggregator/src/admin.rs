use crate::{auth, storage};
use soroban_sdk::{Address, BytesN, Env};

/// Initialize the contract with an admin address.
/// Must be called once after deployment.
pub fn initialize(env: Env, admin: Address) {
    if storage::has_admin(&env) {
        panic!("Already initialized");
    }
    storage::set_admin(&env, &admin);
}


/// Upgrade the contract WASM code. Only admin can call.
pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
    auth::require_admin(&env);
    env.deployer().update_current_contract_wasm(new_wasm_hash);
}


/// Get the admin address.
pub fn admin(env: Env) -> Address {
    storage::get_admin(&env)
}


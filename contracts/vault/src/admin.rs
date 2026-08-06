use crate::{auth, storage};
use soroban_sdk::{token, Address, BytesN, Env};

pub fn initialize(env: Env, admin: Address) {
    if storage::has_admin(&env) {
        panic!("Already initialized");
    }
    storage::set_admin(&env, &admin);
}

pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
    let _admin = auth::require_admin(&env);
    env.deployer().update_current_contract_wasm(new_wasm_hash);
}

pub fn admin(env: Env) -> Address {
    storage::get_admin(&env)
}

pub fn add_caller(env: Env, caller: Address) {
    let _admin = auth::require_admin(&env);
    storage::set_caller(&env, &caller, true);
}

pub fn remove_caller(env: Env, caller: Address) {
    let _admin = auth::require_admin(&env);
    storage::set_caller(&env, &caller, false);
}

pub fn is_caller(env: Env, caller: Address) -> bool {
    storage::is_caller(&env, &caller)
}

pub fn admin_withdraw(env: Env, token: Address, to: Address, amount: i128) {
    let _admin = auth::require_admin(&env);
    assert!(amount > 0, "amount must be positive");
    let vault = env.current_contract_address();
    token::Client::new(&env, &token).transfer(&vault, &to, &amount);
}

use {
    crate::{auth, storage},
    lumagg_contract_types::DexType,
    soroban_sdk::{Address, BytesN, Env},
};

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

pub fn set_venue(env: Env, dex_type: DexType, dex_id: Address, allowed: bool) {
    auth::require_admin(&env);
    storage::set_venue(&env, dex_tag(&dex_type), &dex_id, allowed);
}

pub fn is_venue(env: Env, dex_type: DexType, dex_id: Address) -> bool {
    storage::is_venue(&env, dex_tag(&dex_type), &dex_id)
}

fn dex_tag(dex_type: &DexType) -> u32 {
    match dex_type {
        DexType::Aquarius => 0,
        DexType::SoroswapPair => 1,
        DexType::Phoenix => 2,
        DexType::Sushi => 3,
        DexType::CometDex => 4,
    }
}

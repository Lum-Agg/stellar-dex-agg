use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Venue(u32, Address),
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).expect("Not initialized")
}

pub fn set_venue(env: &Env, dex_tag: u32, dex_id: &Address, allowed: bool) {
    let key = DataKey::Venue(dex_tag, dex_id.clone());
    if allowed {
        env.storage().persistent().set(&key, &true);
    } else {
        env.storage().persistent().remove(&key);
    }
}

pub fn is_venue(env: &Env, dex_tag: u32, dex_id: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Venue(dex_tag, dex_id.clone()))
        .unwrap_or(false)
}

use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Caller(Address),
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

pub fn set_caller(env: &Env, caller: &Address, allowed: bool) {
    if allowed {
        env.storage().persistent().set(&DataKey::Caller(caller.clone()), &true);
    } else {
        env.storage().persistent().remove(&DataKey::Caller(caller.clone()));
    }
}

pub fn is_caller(env: &Env, caller: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Caller(caller.clone()))
        .unwrap_or(false)
}

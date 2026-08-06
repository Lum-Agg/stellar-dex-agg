use crate::storage;
use soroban_sdk::{Address, Env};

pub fn require_admin(env: &Env) -> Address {
    let admin = storage::get_admin(env);
    admin.require_auth();
    admin
}

pub fn require_caller(env: &Env, caller: &Address) {
    caller.require_auth();
    assert!(storage::is_caller(env, caller), "caller not authorized");
}

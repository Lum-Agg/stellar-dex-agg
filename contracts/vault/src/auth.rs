use {
    crate::{errors::VaultError, storage},
    soroban_sdk::{Address, Env},
};

pub fn require_admin(env: &Env) -> Address {
    let admin = storage::get_admin(env);
    admin.require_auth();
    admin
}

pub fn require_caller(env: &Env, caller: &Address) {
    caller.require_auth();
    soroban_sdk::assert_with_error!(env, storage::is_caller(env, caller), VaultError::UnauthorizedCaller);
}

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

use {
    lumagg_contract_types::SubRoute,
    soroban_sdk::{contract, contractclient, contractimpl, contracttype, token, Address, BytesN, Env, Vec},
};

#[contractclient(name = "AggregatorContractClient")]
pub trait AggregatorContract {
    fn round_trip_swap(
        env: Env,
        user: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128;
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Caller(Address),
}

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).expect("Not initialized")
    }

    pub fn add_caller(env: Env, caller: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Caller(caller.clone()), &true);
    }

    pub fn remove_caller(env: Env, caller: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();
        env.storage().persistent().remove(&DataKey::Caller(caller));
    }

    pub fn is_caller(env: Env, caller: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Caller(caller))
            .unwrap_or(false)
    }

    /// Pull tokens from `from` into the vault (any account may fund the vault).
    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");
        let vault = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&from, &vault, &amount);
    }

    /// Admin emergency withdrawal from vault balances.
    pub fn admin_withdraw(env: Env, token: Address, to: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();
        assert!(amount > 0, "amount must be positive");
        let vault = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&vault, &to, &amount);
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
        caller.require_auth();
        assert!(Self::is_caller(env.clone(), caller.clone()), "caller not authorized");
        assert!(amount_in > 0, "amount_in must be positive");
        assert!(min_amount_out >= amount_in, "min_amount_out below principal");
        assert!(
            allowance_expiration_ledger >= env.ledger().sequence(),
            "allowance expiration in the past"
        );

        let vault = env.current_contract_address();
        let base_client = token::Client::new(&env, &base_token);

        // Fixed-ceiling approve + transfer_from(actual): amount is not pinned
        // to simulated `base_total` (that caused auth invalid_action).
        // Expiration comes from the call arg above — do NOT replace with
        // `env.ledger().sequence().saturating_add(N)` (Unauthorized on
        // inclusion; see crate docs).
        base_client.approve(&caller, &vault, &i128::MAX, &allowance_expiration_ledger);

        base_client.transfer(&vault, &caller, &amount_in);

        let agg = AggregatorContractClient::new(&env, &aggregator);
        let base_total = agg.round_trip_swap(
            &caller,
            &base_token,
            &bridge_token,
            &amount_in,
            &leg_out,
            &leg_back,
            &min_amount_out,
        );

        base_client.transfer_from(&vault, &caller, &vault, &base_total);

        base_total
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        aggregator_contract::AggregatorContract,
        lumagg_contract_types::{DexType, SubRoute, SwapStep},
        soroban_sdk::{testutils::Address as _, vec, Address, Env},
    };

    fn gen_addr(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_token(env: &Env) -> (Address, soroban_sdk::token::StellarAssetClient<'static>) {
        let admin = gen_addr(env);
        let addr = env.register_stellar_asset_contract_v2(admin).address();
        let sac = soroban_sdk::token::StellarAssetClient::new(env, &addr);
        (addr, sac)
    }

    fn setup_agg(env: &Env) -> aggregator_contract::AggregatorContractClient<'_> {
        let id = env.register_contract(None, AggregatorContract);
        let agg = aggregator_contract::AggregatorContractClient::new(env, &id);
        agg.initialize(&gen_addr(env));
        agg
    }

    fn setup_vault<'a>(env: &'a Env, admin: &Address) -> (Address, VaultContractClient<'a>) {
        let id = env.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(env, &id);
        vault.initialize(admin);
        (id, vault)
    }

    // ── Mock Aquarius Pool (1:1 swap) ──
    mod aq_mock {
        use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

        #[contract]
        pub struct AqPool;

        #[contracttype]
        enum AqKey {
            TokenA,
            TokenB,
        }

        #[contractimpl]
        impl AqPool {
            pub fn init(env: Env, a: Address, b: Address) {
                env.storage().instance().set(&AqKey::TokenA, &a);
                env.storage().instance().set(&AqKey::TokenB, &b);
            }
            pub fn swap(env: Env, user: Address, in_idx: u32, out_idx: u32, in_amount: u128, _min: u128) -> u128 {
                user.require_auth();
                let a: Address = env.storage().instance().get(&AqKey::TokenA).unwrap();
                let b: Address = env.storage().instance().get(&AqKey::TokenB).unwrap();
                let in_t = if in_idx == 0 { &a } else { &b };
                let out_t = if out_idx == 0 { &a } else { &b };
                let me = env.current_contract_address();
                token::Client::new(&env, in_t).transfer(&user, &me, &(in_amount as i128));
                token::Client::new(&env, out_t).transfer(&me, &user, &(in_amount as i128));
                in_amount
            }
        }
    }

    #[test]
    fn execute_round_trip_returns_funds_to_vault() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = gen_addr(&env);
        let caller = gen_addr(&env);
        let funder = gen_addr(&env);
        let agg = setup_agg(&env);
        let (vault_id, vault) = setup_vault(&env, &admin);
        vault.add_caller(&caller);

        let (base, sac_base) = create_token(&env);
        let (bridge, sac_bridge) = create_token(&env);
        sac_base.mint(&funder, &1_000_000);
        vault.deposit(&funder, &base, &100_000);

        let out_pid = env.register_contract(None, aq_mock::AqPool);
        let out_pool = out_pid.clone();
        aq_mock::AqPoolClient::new(&env, &out_pid).init(&base, &bridge);
        sac_bridge.mint(&out_pool, &10_000_000);

        let back_pid = env.register_contract(None, aq_mock::AqPool);
        let back_pool = back_pid.clone();
        aq_mock::AqPoolClient::new(&env, &back_pid).init(&bridge, &base);
        sac_base.mint(&back_pool, &10_000_000);

        let leg_out = vec![
            &env,
            SubRoute {
                amount_in: 5000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: out_pool,
                        dex_type: DexType::Aquarius,
                        token_in: base.clone(),
                        token_out: bridge.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];
        let leg_back = vec![
            &env,
            SubRoute {
                amount_in: 5000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: back_pool,
                        dex_type: DexType::Aquarius,
                        token_in: bridge.clone(),
                        token_out: base.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];

        let vault_before = token::Client::new(&env, &base).balance(&vault_id);
        let allowance_exp = env.ledger().sequence().saturating_add(10_000);
        let out = vault.execute_round_trip(
            &caller,
            &agg.address,
            &base,
            &bridge,
            &5000,
            &leg_out,
            &leg_back,
            &5000,
            &allowance_exp,
        );
        assert_eq!(out, 5000);
        assert_eq!(token::Client::new(&env, &base).balance(&vault_id), vault_before);
    }

    #[test]
    fn non_caller_cannot_execute() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let admin = gen_addr(&env);
        let caller = gen_addr(&env);
        let stranger = gen_addr(&env);
        let agg = setup_agg(&env);
        let (_, vault) = setup_vault(&env, &admin);
        vault.add_caller(&caller);

        let (base, _) = create_token(&env);
        let (bridge, _) = create_token(&env);
        let empty_legs = vec![&env];

        let allowance_exp = env.ledger().sequence().saturating_add(10_000);
        let result = vault.try_execute_round_trip(
            &stranger,
            &agg.address,
            &base,
            &bridge,
            &1000,
            &empty_legs,
            &empty_legs,
            &1000,
            &allowance_exp,
        );
        assert!(result.is_err());
    }
}

use {
    super::*,
    aggregator_contract::AggregatorContract,
    lumagg_contract_types::{DexType, SubRoute, SwapStep},
    soroban_sdk::{testutils::Address as _, token, vec, Address, Env},
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
    let id = env.register(AggregatorContract, ());
    let agg = aggregator_contract::AggregatorContractClient::new(env, &id);
    agg.initialize(&gen_addr(env));
    agg
}

fn setup_vault<'a>(env: &'a Env, admin: &Address) -> (Address, VaultContractClient<'a>) {
    let id = env.register(VaultContract, ());
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

    let out_pid = env.register(aq_mock::AqPool, ());
    let out_pool = out_pid.clone();
    aq_mock::AqPoolClient::new(&env, &out_pid).init(&base, &bridge);
    sac_bridge.mint(&out_pool, &10_000_000);

    let back_pid = env.register(aq_mock::AqPool, ());
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

#![no_std]

use {
    lumagg_contract_types::SubRoute,
    soroban_sdk::{
        auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
        contract, contractclient, contractimpl, contracttype, Address, Env, IntoVal, Symbol, Vec,
    },
};

#[contractclient(name = "AggregatorContractClient")]
pub trait AggregatorContract {
    fn swap(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128;
}

#[contracttype]
enum DataKey {
    Admin,
}

#[contract]
pub struct OrderEscrowContract;

#[contractimpl]
impl OrderEscrowContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Temporary auth probe for Task 1. Limit-order ABI follows in Task 2.
    pub fn spike_swap_as_self(
        env: Env,
        aggregator: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        let escrow = env.current_contract_address();
        let mut amount_in = 0i128;
        for route in sub_routes.iter() {
            amount_in += route.amount_in;
        }

        env.authorize_as_current_contract(soroban_sdk::vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: aggregator.clone(),
                    fn_name: Symbol::new(&env, "swap"),
                    args: soroban_sdk::vec![
                        &env,
                        escrow.clone().into_val(&env),
                        token_in.clone().into_val(&env),
                        token_out.clone().into_val(&env),
                        sub_routes.clone().into_val(&env),
                        min_amount_out.into_val(&env),
                    ],
                },
                sub_invocations: soroban_sdk::vec![
                    &env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: token_in.clone(),
                            fn_name: Symbol::new(&env, "transfer"),
                            args: soroban_sdk::vec![
                                &env,
                                escrow.clone().into_val(&env),
                                aggregator.clone().into_val(&env),
                                amount_in.into_val(&env),
                            ],
                        },
                        sub_invocations: soroban_sdk::vec![&env],
                    }),
                ],
            }),
        ]);

        AggregatorContractClient::new(&env, &aggregator).swap(
            &escrow,
            &token_in,
            &token_out,
            &sub_routes,
            &min_amount_out,
        )
    }
}

#[cfg(test)]
mod tests {
    use {
        super::{OrderEscrowContract, OrderEscrowContractClient},
        aggregator_contract::AggregatorContract,
        lumagg_contract_types::{DexType, SubRoute, SwapStep},
        soroban_sdk::{testutils::{Address as _, EnvTestConfig}, token, vec, Address, Env},
    };

    fn gen_addr(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_token(env: &Env) -> (Address, token::StellarAssetClient<'static>) {
        let admin = gen_addr(env);
        let address = env.register_stellar_asset_contract_v2(admin).address();
        let sac = token::StellarAssetClient::new(env, &address);
        (address, sac)
    }

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
            pub fn init(env: Env, token_a: Address, token_b: Address) {
                env.storage().instance().set(&AqKey::TokenA, &token_a);
                env.storage().instance().set(&AqKey::TokenB, &token_b);
            }

            pub fn swap(env: Env, user: Address, in_idx: u32, out_idx: u32, amount_in: u128, _min: u128) -> u128 {
                user.require_auth();
                let token_a: Address = env.storage().instance().get(&AqKey::TokenA).unwrap();
                let token_b: Address = env.storage().instance().get(&AqKey::TokenB).unwrap();
                let token_in = if in_idx == 0 { &token_a } else { &token_b };
                let token_out = if out_idx == 0 { &token_a } else { &token_b };
                let pool = env.current_contract_address();
                token::Client::new(&env, token_in).transfer(&user, &pool, &(amount_in as i128));
                token::Client::new(&env, token_out).transfer(&pool, &user, &(amount_in as i128));
                amount_in
            }
        }
    }

    #[test]
    fn spike_escrow_can_be_aggregator_user() {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths_allowing_non_root_auth();

        let aggregator_id = env.register_contract(None, AggregatorContract);
        let aggregator = aggregator_contract::AggregatorContractClient::new(&env, &aggregator_id);
        aggregator.initialize(&gen_addr(&env));

        let escrow_id = env.register_contract(None, OrderEscrowContract);
        let escrow = OrderEscrowContractClient::new(&env, &escrow_id);
        escrow.initialize(&gen_addr(&env));
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, token_out_sac) = create_token(&env);
        token_in_sac.mint(&escrow_id, &5_000);

        let pool_id = env.register_contract(None, aq_mock::AqPool);
        aq_mock::AqPoolClient::new(&env, &pool_id).init(&token_in, &token_out);
        token_out_sac.mint(&pool_id, &5_000);

        let routes = vec![
            &env,
            SubRoute {
                amount_in: 5_000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: pool_id,
                        dex_type: DexType::Aquarius,
                        token_in: token_in.clone(),
                        token_out: token_out.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];

        let out = escrow.spike_swap_as_self(&aggregator_id, &token_in, &token_out, &routes, &5_000);
        assert_eq!(out, 5_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
        assert_eq!(token::Client::new(&env, &token_out).balance(&escrow_id), 5_000);
    }
}

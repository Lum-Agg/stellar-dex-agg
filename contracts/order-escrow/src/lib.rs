#![no_std]

use {
    lumagg_contract_types::SubRoute,
    soroban_sdk::{
        auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
        contract, contractclient, contractimpl, contracttype, token, Address, Env, IntoVal, Symbol, Vec,
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
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Aggregator,
    NextOrderId,
    Order(u64),
}

#[contracttype]
#[derive(Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct LimitOrder {
    pub owner: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in_remaining: i128,
    pub limit_out_per_in_e7: i128,
    pub expires_ledger: u32,
    pub status: OrderStatus,
}

#[contract]
pub struct OrderEscrowContract;

const RATE_SCALE_E7: i128 = 10_000_000;

fn required_min_out(amount_in: i128, limit_out_per_in_e7: i128) -> i128 {
    assert!(amount_in >= 0, "amount_in must not be negative");
    assert!(limit_out_per_in_e7 > 0, "limit must be positive");
    amount_in
        .checked_mul(limit_out_per_in_e7)
        .expect("amount and limit multiplication overflow")
        / RATE_SCALE_E7
}

fn authorize_swap_as_current_contract(
    env: &Env,
    aggregator: &Address,
    escrow: &Address,
    token_in: &Address,
    token_out: &Address,
    sub_routes: &Vec<SubRoute>,
    min_amount_out: i128,
    amount_in: i128,
) {
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: aggregator.clone(),
                fn_name: Symbol::new(env, "swap"),
                args: soroban_sdk::vec![
                    env,
                    escrow.clone().into_val(env),
                    token_in.clone().into_val(env),
                    token_out.clone().into_val(env),
                    sub_routes.clone().into_val(env),
                    min_amount_out.into_val(env),
                ],
            },
            sub_invocations: soroban_sdk::vec![
                env,
                InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {
                        contract: token_in.clone(),
                        fn_name: Symbol::new(env, "transfer"),
                        args: soroban_sdk::vec![
                            env,
                            escrow.clone().into_val(env),
                            aggregator.clone().into_val(env),
                            amount_in.into_val(env),
                        ],
                    },
                    sub_invocations: soroban_sdk::vec![env],
                }),
            ],
        }),
    ]);
}

#[contractimpl]
impl OrderEscrowContract {
    pub fn initialize(env: Env, admin: Address, aggregator: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Aggregator, &aggregator);
        env.storage().instance().set(&DataKey::NextOrderId, &0u64);
    }

    pub fn create_limit(
        env: Env,
        owner: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        limit_out_per_in_e7: i128,
        expires_ledger: u32,
    ) -> u64 {
        owner.require_auth();
        assert!(amount_in > 0, "amount_in must be positive");
        assert!(limit_out_per_in_e7 > 0, "limit must be positive");
        assert!(token_in != token_out, "tokens must differ");
        assert!(
            expires_ledger > env.ledger().sequence(),
            "expiration must be in the future"
        );

        let order_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextOrderId)
            .expect("Not initialized");
        let escrow = env.current_contract_address();
        token::Client::new(&env, &token_in).transfer(&owner, &escrow, &amount_in);

        let order = LimitOrder {
            owner,
            token_in,
            token_out,
            amount_in_remaining: amount_in,
            limit_out_per_in_e7,
            expires_ledger,
            status: OrderStatus::Open,
        };
        env.storage().persistent().set(&DataKey::Order(order_id), &order);
        env.storage().instance().set(
            &DataKey::NextOrderId,
            &order_id.checked_add(1).expect("order id overflow"),
        );
        env.events().publish(
            (Symbol::new(&env, "order_created"), order_id),
            (
                order.owner.clone(),
                order.token_in.clone(),
                order.token_out.clone(),
                amount_in,
                limit_out_per_in_e7,
                expires_ledger,
            ),
        );
        order_id
    }

    pub fn cancel(env: Env, order_id: u64) {
        let key = DataKey::Order(order_id);
        let mut order: LimitOrder = env.storage().persistent().get(&key).expect("Order not found");
        order.owner.require_auth();
        assert!(order.status == OrderStatus::Open, "Order is not open");

        let escrow = env.current_contract_address();
        let refunded_amount = order.amount_in_remaining;
        token::Client::new(&env, &order.token_in).transfer(&escrow, &order.owner, &refunded_amount);
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_cancelled"), order_id),
            (order.owner.clone(), refunded_amount),
        );
    }

    pub fn reclaim_expired(env: Env, order_id: u64) {
        let key = DataKey::Order(order_id);
        let mut order: LimitOrder = env.storage().persistent().get(&key).expect("Order not found");
        assert!(order.status == OrderStatus::Open, "Order is not open");
        assert!(
            env.ledger().sequence() > order.expires_ledger,
            "Order has not expired"
        );

        let escrow = env.current_contract_address();
        let refunded_amount = order.amount_in_remaining;
        token::Client::new(&env, &order.token_in).transfer(
            &escrow,
            &order.owner,
            &refunded_amount,
        );
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Expired;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_expired"), order_id),
            (order.owner.clone(), refunded_amount),
        );
    }

    pub fn fill(env: Env, order_id: u64, amount_in: i128, sub_routes: Vec<SubRoute>, min_amount_out: i128) -> i128 {
        let key = DataKey::Order(order_id);
        let mut order: LimitOrder = env.storage().persistent().get(&key).expect("Order not found");
        assert!(order.status == OrderStatus::Open, "Order is not open");
        assert!(env.ledger().sequence() < order.expires_ledger, "Order is expired");
        assert!(amount_in > 0, "amount_in must be positive");
        assert!(amount_in <= order.amount_in_remaining, "amount_in exceeds remaining");

        let mut routed_amount = 0i128;
        for route in sub_routes.iter() {
            routed_amount = routed_amount
                .checked_add(route.amount_in)
                .expect("sub-route amount overflow");
        }
        assert!(routed_amount == amount_in, "sub-route amounts must equal amount_in");
        assert!(
            min_amount_out >= required_min_out(amount_in, order.limit_out_per_in_e7),
            "min_amount_out below limit"
        );

        let aggregator: Address = env
            .storage()
            .instance()
            .get(&DataKey::Aggregator)
            .expect("Not initialized");
        let escrow = env.current_contract_address();
        authorize_swap_as_current_contract(
            &env,
            &aggregator,
            &escrow,
            &order.token_in,
            &order.token_out,
            &sub_routes,
            min_amount_out,
            amount_in,
        );
        let amount_out = AggregatorContractClient::new(&env, &aggregator).swap(
            &escrow,
            &order.token_in,
            &order.token_out,
            &sub_routes,
            &min_amount_out,
        );
        token::Client::new(&env, &order.token_out).transfer(&escrow, &order.owner, &amount_out);

        order.amount_in_remaining -= amount_in;
        if order.amount_in_remaining == 0 {
            order.status = OrderStatus::Filled;
        }
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_filled"), order_id),
            (order.owner, amount_in, amount_out, order.amount_in_remaining),
        );
        amount_out
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

        authorize_swap_as_current_contract(
            &env,
            &aggregator,
            &escrow,
            &token_in,
            &token_out,
            &sub_routes,
            min_amount_out,
            amount_in,
        );

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
        super::{required_min_out, DataKey, LimitOrder, OrderEscrowContract, OrderEscrowContractClient, OrderStatus},
        aggregator_contract::AggregatorContract,
        lumagg_contract_types::{DexType, SubRoute, SwapStep},
        soroban_sdk::{
            contract, contractimpl,
            testutils::{Address as _, EnvTestConfig, Events as _, Ledger, LedgerInfo},
            token, vec, Address, Env, Symbol, TryFromVal, Vec,
        },
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

    fn test_env() -> Env {
        Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        })
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

    #[contract]
    struct Filler;

    #[contractimpl]
    impl Filler {
        pub fn fill(
            env: Env,
            escrow: Address,
            order_id: u64,
            amount_in: i128,
            sub_routes: Vec<SubRoute>,
            min_amount_out: i128,
        ) -> i128 {
            OrderEscrowContractClient::new(&env, &escrow).fill(&order_id, &amount_in, &sub_routes, &min_amount_out)
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
        escrow.initialize(&gen_addr(&env), &aggregator_id);
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

    #[test]
    fn min_out_scales_with_fill_size() {
        // A limit of 2e7 means two output stroops per input stroop.
        assert_eq!(required_min_out(5_000_000, 20_000_000), 10_000_000);
    }

    #[test]
    fn min_out_zero_and_overflow_guards() {
        assert_eq!(required_min_out(0, 20_000_000), 0);
    }

    fn setup_escrow<'a>(env: &'a Env) -> (Address, OrderEscrowContractClient<'a>) {
        let escrow_id = env.register_contract(None, OrderEscrowContract);
        let escrow = OrderEscrowContractClient::new(env, &escrow_id);
        escrow.initialize(&gen_addr(env), &gen_addr(env));
        (escrow_id, escrow)
    }

    fn setup_fill<'a>(
        env: &'a Env,
    ) -> (
        Address,
        OrderEscrowContractClient<'a>,
        Address,
        Address,
        token::StellarAssetClient<'a>,
        Address,
    ) {
        let owner = gen_addr(env);
        let aggregator_id = env.register_contract(None, AggregatorContract);
        aggregator_contract::AggregatorContractClient::new(env, &aggregator_id).initialize(&gen_addr(env));

        let escrow_id = env.register_contract(None, OrderEscrowContract);
        let escrow = OrderEscrowContractClient::new(env, &escrow_id);
        escrow.initialize(&gen_addr(env), &aggregator_id);

        let (token_in, token_in_sac) = create_token(env);
        let (token_out, token_out_sac) = create_token(env);
        token_in_sac.mint(&owner, &10_000);

        let pool_id = env.register_contract(None, aq_mock::AqPool);
        aq_mock::AqPoolClient::new(env, &pool_id).init(&token_in, &token_out);
        token_out_sac.mint(&pool_id, &10_000);

        (owner, escrow, token_in, token_out, token_in_sac, pool_id)
    }

    fn route(env: &Env, pool_id: &Address, token_in: &Address, token_out: &Address, amount_in: i128) -> Vec<SubRoute> {
        vec![
            env,
            SubRoute {
                amount_in,
                steps: vec![
                    env,
                    SwapStep {
                        dex_id: pool_id.clone(),
                        dex_type: DexType::Aquarius,
                        token_in: token_in.clone(),
                        token_out: token_out.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ]
    }

    fn order(env: &Env, escrow: &Address, order_id: u64) -> LimitOrder {
        env.as_contract(escrow, || {
            env.storage()
                .persistent()
                .get::<_, LimitOrder>(&DataKey::Order(order_id))
                .unwrap()
        })
    }

    fn has_lifecycle_event(env: &Env, escrow_id: &Address, topic_name: &str, order_id: u64) -> bool {
        let expected_topic = Symbol::new(env, topic_name);
        env.events().all().iter().any(|(contract, topics, _data)| {
            if contract != *escrow_id || topics.len() < 2 {
                return false;
            }
            Symbol::try_from_val(env, &topics.get(0).unwrap()) == Ok(expected_topic.clone())
                && u64::try_from_val(env, &topics.get(1).unwrap()) == Ok(order_id)
        })
    }

    #[test]
    fn create_limit_emits_order_created_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);

        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_created", order_id));
    }

    #[test]
    fn cancel_emits_order_cancelled_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        escrow.cancel(&order_id);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_cancelled", order_id));
    }

    #[test]
    fn reclaim_expired_emits_order_expired_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_expired", order_id));
    }

    #[test]
    fn create_limit_pulls_token_in() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);

        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        assert_eq!(order_id, 0);
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 0);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
    }

    #[test]
    fn owner_can_cancel_and_refund() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        escrow.cancel(&order_id);

        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
    }

    #[test]
    fn reclaim_expired_refunds_open_residual() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
        let order = order(&env, &escrow_id, order_id);
        assert_eq!(order.amount_in_remaining, 0);
        assert!(order.status == OrderStatus::Expired);
    }

    #[test]
    fn reclaim_expired_rejects_second_reclaim() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (_, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        let result = escrow.try_reclaim_expired(&order_id);
        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
    }

    #[test]
    fn reclaim_expired_rejects_before_expiry() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        let result = escrow.try_reclaim_expired(&order_id);

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
        assert!(order(&env, &escrow_id, order_id).status == OrderStatus::Open);
    }

    #[test]
    fn non_owner_cannot_cancel() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_token(&env);
        let (token_out, _) = create_token(&env);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        // A caller who cannot supply the owner's authorization must not be able
        // to cancel the order.
        env.set_auths(&[]);
        let result = escrow.try_cancel(&order_id);
        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
    }

    #[test]
    fn fill_executes_when_limit_met() {
        let env = test_env();
        env.mock_all_auths_allowing_non_root_auth();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);

        let out = escrow.fill(
            &order_id,
            &5_000,
            &route(&env, &pool_id, &token_in, &token_out, 5_000),
            &5_000,
        );

        assert_eq!(out, 5_000);
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 5_000);
        let order = order(&env, &escrow.address, order_id);
        assert_eq!(order.amount_in_remaining, 0);
        assert!(order.status == OrderStatus::Filled);
    }

    #[test]
    fn fill_rejects_when_min_out_below_limit() {
        let env = test_env();
        env.mock_all_auths_allowing_non_root_auth();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);

        let result = escrow.try_fill(
            &order_id,
            &5_000,
            &route(&env, &pool_id, &token_in, &token_out, 5_000),
            &4_999,
        );

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 5_000);
    }

    #[test]
    fn fill_rejects_expired() {
        let env = test_env();
        env.mock_all_auths_allowing_non_root_auth();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 100,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        let result = escrow.try_fill(
            &order_id,
            &5_000,
            &route(&env, &pool_id, &token_in, &token_out, 5_000),
            &5_000,
        );

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
    }

    #[test]
    fn partial_fill_reduces_remaining() {
        let env = test_env();
        env.mock_all_auths_allowing_non_root_auth();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);

        assert_eq!(
            escrow.fill(
                &order_id,
                &2_000,
                &route(&env, &pool_id, &token_in, &token_out, 2_000),
                &2_000
            ),
            2_000
        );

        let order = order(&env, &escrow.address, order_id);
        assert_eq!(order.amount_in_remaining, 3_000);
        assert!(order.status == OrderStatus::Open);
    }

    #[test]
    fn anyone_can_fill() {
        let env = test_env();
        env.mock_all_auths_allowing_non_root_auth();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let filler = env.register_contract(None, Filler);
        assert_ne!(filler, owner);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);

        assert_eq!(
            FillerClient::new(&env, &filler).fill(
                &escrow.address,
                &order_id,
                &5_000,
                &route(&env, &pool_id, &token_in, &token_out, 5_000),
                &5_000
            ),
            5_000
        );
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 5_000);
    }
}

#![no_std]
//! Stellar DEX Aggregator Contract
//!
//! Executes multi-hop and split-order swaps atomically across Soroban DEXes
//! (Aquarius, Soroswap, Phoenix, Sushi V3, Comet).
//!
//! Main entry point:
//! - `swap()`: Atomic swap via `sub_routes` (one leg = simple path; multiple =
//!   split)
//!
//! Key design: the contract holds no funds permanently.
//! Users approve token transfers, the contract executes swaps, and outputs go
//! directly back to the user — all in one atomic invocation.

mod auth;
mod events;
mod math;
mod storage;
mod validate;

pub use lumagg_contract_types::{DexType, SubRoute, SwapStep};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, Symbol, Val, Vec,
};

#[contract]
pub struct AggregatorContract;

#[contractimpl]
impl AggregatorContract {
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

    /// Execute a swap atomically (single-path or split-order).
    ///
    /// `sub_routes` is always a list of legs; a simple swap is one entry with
    /// the full `amount_in` and its hop `steps`. Split execution uses
    /// multiple entries.
    ///
    /// Flow:
    /// 1. Pull total input from user (sum of sub-route amounts)
    /// 2. For each sub-route: execute its path with its allocated amount
    /// 3. Sum outputs (all must produce the same `token_out`)
    /// 4. Verify total output >= `min_amount_out`
    /// 5. Transfer total output to user
    pub fn swap(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();
        assert!(token_in != token_out, "tokens must differ");
        assert!(min_amount_out > 0, "min_amount_out must be positive");

        let contract_addr = env.current_contract_address();
        let total_in = validate::validate_sub_routes(&token_in, &token_out, &sub_routes);

        // Pull total input from user
        let token_in_client = token::Client::new(&env, &token_in);
        token_in_client.transfer(&user, &contract_addr, &total_in);

        let mut leg_counter: u32 = 0;
        let total_output = Self::execute_sub_routes(&env, &sub_routes, &contract_addr, &mut leg_counter);

        // Slippage: per-hop pool mins are 0; only check total output here (all
        // sub_routes summed).
        assert!(total_output >= min_amount_out, "Output below minimum");

        // Transfer total output to user
        let token_out_client = token::Client::new(&env, &token_out);
        token_out_client.transfer(&contract_addr, &user, &total_output);

        events::publish_swap(
            &env,
            &user,
            &token_in,
            &token_out,
            total_in,
            total_output,
            sub_routes.len() as u32,
        );

        total_output
    }

    /// Round-trip swap: base → bridge (split OK) → base (split OK) in one
    /// atomic invocation.
    ///
    /// Funds are pulled from `user` and the final `base_token` balance is
    /// returned to `user`. The contract does not retain funds after
    /// execution.
    ///
    /// # Parameters
    ///
    /// - `leg_out`: sub-routes from `base_token` to `bridge_token`. Each
    ///   `SubRoute.amount_in` is an absolute base-token input; they **must**
    ///   sum to `amount_in`.
    /// - `leg_back`: sub-routes from `bridge_token` to `base_token`. Each
    ///   `SubRoute.amount_in` is a **positive weight** (quoted bridge amounts
    ///   are fine). After `leg_out` produces actual bridge total `o1`, weights
    ///   are rescaled so executed inputs sum **exactly** to `o1` (last
    ///   sub-route receives the remainder). Callers do **not** need to know
    ///   `o1` at submit time.
    /// - `min_amount_out`: minimum total `base_token` returned (principal +
    ///   profit floor)
    ///
    /// # Integrator note
    ///
    /// Same `SubRoute` type for both legs — no extra fields. Semantics of
    /// `amount_in` differ by leg: absolute on `leg_out`, proportional weight
    /// on `leg_back`.
    pub fn round_trip_swap(
        env: Env,
        user: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();
        assert!(amount_in > 0, "amount_in must be positive");
        assert!(min_amount_out >= amount_in, "min_amount_out below principal");
        assert!(base_token != bridge_token, "base and bridge must differ");

        let contract_addr = env.current_contract_address();

        let mut leg_counter: u32 = 0;

        let leg_out_in = validate::validate_sub_routes(&base_token, &bridge_token, &leg_out);
        validate::validate_sub_routes(&bridge_token, &base_token, &leg_back);
        assert!(leg_out_in == amount_in, "leg_out amounts must sum to amount_in");
        let is_split = leg_out.len() > 1 || leg_back.len() > 1;

        // Pull base from user
        let base_client = token::Client::new(&env, &base_token);
        base_client.transfer(&user, &contract_addr, &amount_in);

        let bridge_total = Self::execute_sub_routes(&env, &leg_out, &contract_addr, &mut leg_counter);
        assert!(bridge_total > 0, "leg_out produced zero bridge token");

        // Scale leg_back weights → absolute bridge inputs that sum to o1.
        let scaled_back = math::scale_sub_routes_to_total(&env, &leg_back, bridge_total);

        let base_total = Self::execute_sub_routes(&env, &scaled_back, &contract_addr, &mut leg_counter);

        assert!(base_total >= min_amount_out, "Output below minimum");

        base_client.transfer(&contract_addr, &user, &base_total);

        events::publish_rt(
            &env,
            &user,
            &base_token,
            &bridge_token,
            amount_in,
            base_total,
            leg_counter,
            is_split,
        );

        base_total
    }

    /// Execute sub-routes that share the same token_in → token_out pair;
    /// returns total output.
    ///
    /// Parallel split paths share hop indices (`path_base + hop`). After all
    /// paths run, `leg_counter` advances by the **serial depth** (longest path
    /// hop count), not by total hop executions. Exact routed volume is derived
    /// from each emitted `leg` event's actual input.
    fn execute_sub_routes(
        env: &Env,
        sub_routes: &Vec<SubRoute>,
        contract_addr: &Address,
        leg_counter: &mut u32,
    ) -> i128 {
        let path_base = *leg_counter;
        let mut max_depth: u32 = 0;
        let mut total_output: i128 = 0;
        for sr in sub_routes.iter() {
            let output = Self::execute_path(env, &sr.steps, sr.amount_in, contract_addr, path_base, &mut max_depth);
            total_output = total_output.checked_add(output).expect("total output overflow");
        }
        *leg_counter = path_base.checked_add(max_depth).expect("leg counter overflow");
        total_output
    }

    /// Comet rounds the approval ledger to avoid simulation vs execution
    /// sequence mismatch.
    fn comet_approval_ledger(env: &Env) -> u32 {
        let seq = env.ledger().sequence();
        (seq / 100_000 + 1) * 100_000
    }

    /// Execute a path (sequence of swap steps) and return the final output
    /// amount.
    fn execute_path(
        env: &Env,
        steps: &Vec<SwapStep>,
        amount_in: i128,
        my_address: &Address,
        path_base: u32,
        max_depth: &mut u32,
    ) -> i128 {
        assert!(amount_in > 0, "Path input must be positive");
        let mut current_amount = amount_in;

        for (i, step) in steps.iter().enumerate() {
            assert!(step.token_in != step.token_out, "Step tokens must differ");
            assert!(step.in_idx != step.out_idx, "Step indices must differ");
            let hop_idx = path_base + i as u32;
            current_amount = Self::execute_step(env, &step, current_amount, my_address, hop_idx);
            assert!(current_amount > 0, "Step output must be positive");
            let depth = (i as u32) + 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
        }

        current_amount
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

    /// Execute a single swap step on the appropriate DEX.
    fn execute_step(env: &Env, step: &SwapStep, amount_in: i128, my_address: &Address, hop_idx: u32) -> i128 {
        let output = Self::execute_step_inner(env, step, amount_in, my_address);
        env.events().publish(
            (Symbol::new(env, "leg"),),
            (
                hop_idx,
                Self::dex_tag(&step.dex_type),
                step.dex_id.clone(),
                step.token_in.clone(),
                amount_in,
            ),
        );
        output
    }

    fn execute_step_inner(env: &Env, step: &SwapStep, amount_in: i128, my_address: &Address) -> i128 {
        match step.dex_type {
            DexType::Aquarius => {
                // Aquarius pool: swap(user, in_idx, out_idx, in_amount, out_min) -> u128
                // The pool pulls token_in via transfer(user, pool, amount); authorize only that
                // transfer (same pattern as stellar-arb arb-contract).
                let (in_idx, out_idx) = (step.in_idx, step.out_idx);
                let aq_in_amount: u128 = amount_in as u128;

                env.authorize_as_current_contract(soroban_sdk::vec![
                    env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: step.token_in.clone(),
                            fn_name: Symbol::new(env, "transfer"),
                            args: soroban_sdk::vec![
                                env,
                                my_address.into_val(env),
                                step.dex_id.into_val(env),
                                amount_in.into_val(env),
                            ],
                        },
                        sub_invocations: soroban_sdk::vec![env],
                    }),
                ]);

                let received: u128 = env.invoke_contract(
                    &step.dex_id,
                    &Symbol::new(env, "swap"),
                    soroban_sdk::vec![
                        env,
                        my_address.into_val(env),
                        in_idx.into_val(env),
                        out_idx.into_val(env),
                        aq_in_amount.into_val(env),
                        0u128.into_val(env),
                    ],
                );
                received as i128
            }

            DexType::SoroswapPair => {
                // Soroswap flash-swap: transfer in, then pair.swap(out0, out1, to).
                // Same flow as stellar-arb (transfer then pair.swap; pair sends output to
                // aggregator).
                let reserves: (i128, i128) =
                    env.invoke_contract(&step.dex_id, &Symbol::new(env, "get_reserves"), soroban_sdk::vec![env]);

                let a2b = step.in_idx == 0 && step.out_idx == 1;
                let (reserve_in, reserve_out) = if a2b {
                    (reserves.0, reserves.1)
                } else {
                    (reserves.1, reserves.0)
                };

                let expected_out = math::soroswap_get_amount_out(amount_in, reserve_in, reserve_out);
                if expected_out <= 0 {
                    return 0;
                }

                let token_in_client = token::Client::new(env, &step.token_in);
                let token_out_client = token::Client::new(env, &step.token_out);
                let balance_before = token_out_client.balance(my_address);

                token_in_client.transfer(my_address, &step.dex_id, &amount_in);

                let (amount0_out, amount1_out): (i128, i128) = if a2b { (0, expected_out) } else { (expected_out, 0) };

                let swap_args = soroban_sdk::vec![
                    env,
                    amount0_out.into_val(env),
                    amount1_out.into_val(env),
                    my_address.into_val(env),
                ];
                let _: Val = env.invoke_contract(&step.dex_id, &Symbol::new(env, "swap"), swap_args);

                let balance_after = token_out_client.balance(my_address);
                balance_after - balance_before
            }

            DexType::Phoenix => {
                // Phoenix: swap(sender, offer_asset, offer_amount, ...)
                // Fee on output, need balance diff to determine actual output
                let token_out_client = token::Client::new(env, &step.token_out);
                let balance_before = token_out_client.balance(my_address);

                let none_val: Val = ().into_val(env);
                let swap_args = soroban_sdk::vec![
                    env,
                    my_address.into_val(env),
                    step.token_in.into_val(env),
                    amount_in.into_val(env),
                    none_val.clone(),
                    none_val.clone(),
                    none_val.clone(),
                    none_val,
                ];

                env.authorize_as_current_contract(soroban_sdk::vec![
                    env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: step.token_in.clone(),
                            fn_name: Symbol::new(env, "transfer"),
                            args: soroban_sdk::vec![
                                env,
                                my_address.into_val(env),
                                step.dex_id.into_val(env),
                                amount_in.into_val(env),
                            ],
                        },
                        sub_invocations: soroban_sdk::vec![env],
                    }),
                ]);

                let _: Val = env.invoke_contract(&step.dex_id, &Symbol::new(env, "swap"), swap_args);

                let balance_after = token_out_client.balance(my_address);
                balance_after - balance_before
            }

            DexType::Sushi => {
                // Sushi V3 pool: swap(sender, recipient, zero_for_one, amount_specified,
                //               sqrt_price_limit_x96, hints)
                // hints must come from get_oracle_hints() on the same pool (see sushiswap
                // bindings).
                let zero_for_one = step.in_idx == 0 && step.out_idx == 1;

                // sqrt_price_limit: MIN_SQRT_RATIO+1 for zero_for_one, MAX_SQRT_RATIO-1
                // otherwise
                let price_limit: soroban_sdk::U256 = if zero_for_one {
                    soroban_sdk::U256::from_u128(env, 4_295_128_740u128)
                } else {
                    soroban_sdk::U256::from_be_bytes(
                        env,
                        &soroban_sdk::Bytes::from_array(
                            env,
                            &[
                                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xfd,
                                0x89, 0x63, 0xef, 0xd1, 0xfc, 0x6a, 0x50, 0x64, 0x88, 0x49, 0x5d, 0x95, 0x1d, 0x52,
                                0x63, 0x98, 0x8d, 0x25,
                            ],
                        ),
                    )
                };

                let hints: Val = env.invoke_contract(
                    &step.dex_id,
                    &Symbol::new(env, "get_oracle_hints"),
                    soroban_sdk::vec![env],
                );

                let token_out_client = token::Client::new(env, &step.token_out);
                let balance_before = token_out_client.balance(my_address);

                let swap_args = soroban_sdk::vec![
                    env,
                    my_address.into_val(env),
                    my_address.into_val(env),
                    zero_for_one.into_val(env),
                    amount_in.into_val(env),
                    price_limit.into_val(env),
                    hints,
                ];

                env.authorize_as_current_contract(soroban_sdk::vec![
                    env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: step.token_in.clone(),
                            fn_name: Symbol::new(env, "transfer"),
                            args: soroban_sdk::vec![
                                env,
                                my_address.into_val(env),
                                step.dex_id.into_val(env),
                                amount_in.into_val(env),
                            ],
                        },
                        sub_invocations: soroban_sdk::vec![env],
                    }),
                ]);

                let _: Val = env.invoke_contract(&step.dex_id, &Symbol::new(env, "swap"), swap_args);

                let balance_after = token_out_client.balance(my_address);
                balance_after - balance_before
            }

            DexType::CometDex => {
                // Comet: swap_exact_amount_in(token_in, amount_in, token_out, min_out,
                // max_price, user). user = aggregator (funds already here).
                //
                // pull_underlying (Comet token_utility) does:
                //   token.approve(from=user, spender=pool, amount, ledger)
                //   token.transfer_from(spender=pool, from=user, to=pool, amount)
                //
                // SAC approve requires auth from `from` (aggregator). transfer_from requires
                // auth from `spender` (the pool), not the aggregator — same pattern as
                // Aquarius/Phoenix flat token.transfer pre-auth before pool.swap.
                let max_price = i128::MAX;
                let approval_ledger = Self::comet_approval_ledger(env);

                env.authorize_as_current_contract(soroban_sdk::vec![
                    env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: step.token_in.clone(),
                            fn_name: Symbol::new(env, "approve"),
                            args: soroban_sdk::vec![
                                env,
                                my_address.into_val(env),
                                step.dex_id.into_val(env),
                                amount_in.into_val(env),
                                approval_ledger.into_val(env),
                            ],
                        },
                        sub_invocations: soroban_sdk::vec![env],
                    }),
                ]);

                let (amount_out, _): (i128, i128) = env.invoke_contract(
                    &step.dex_id,
                    &Symbol::new(env, "swap_exact_amount_in"),
                    soroban_sdk::vec![
                        env,
                        step.token_in.into_val(env),
                        amount_in.into_val(env),
                        step.token_out.into_val(env),
                        0i128.into_val(env),
                        max_price.into_val(env),
                        my_address.into_val(env),
                    ],
                );
                amount_out
            }
        }
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use {
        super::*,
        soroban_sdk::{
            testutils,
            token::{StellarAssetClient, TokenClient},
            vec, Address, Env,
        },
    };

    fn gen_addr(env: &Env) -> Address {
        <Address as testutils::Address>::generate(env)
    }

    fn create_token(env: &Env) -> (Address, StellarAssetClient<'static>, TokenClient<'static>) {
        let admin = gen_addr(env);
        let addr = env.register_stellar_asset_contract_v2(admin).address();
        let sac = StellarAssetClient::new(env, &addr);
        let tok = TokenClient::new(env, &addr);
        (addr, sac, tok)
    }

    fn setup_agg(env: &Env) -> (Address, AggregatorContractClient<'_>) {
        let admin = gen_addr(env);
        let id = env.register_contract(None, AggregatorContract);
        let agg = AggregatorContractClient::new(env, &id);
        agg.initialize(&admin);
        (admin, agg)
    }

    fn single_swap(
        env: &Env,
        agg: &AggregatorContractClient<'_>,
        user: &Address,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
        steps: soroban_sdk::Vec<SwapStep>,
        min: i128,
    ) -> i128 {
        let sub = SubRoute { amount_in, steps };
        agg.swap(user, token_in, token_out, &vec![env, sub], &min)
    }

    // ── Mock Aquarius Pool ──
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

    // ── Mock Soroswap Pair ──
    mod ss_mock {
        use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

        #[contract]
        pub struct SsPair;

        #[contracttype]
        enum SsKey {
            TokenA,
            TokenB,
            ReserveA,
            ReserveB,
        }

        #[contractimpl]
        impl SsPair {
            pub fn init(env: Env, a: Address, b: Address, ra: i128, rb: i128) {
                env.storage().instance().set(&SsKey::TokenA, &a);
                env.storage().instance().set(&SsKey::TokenB, &b);
                env.storage().instance().set(&SsKey::ReserveA, &ra);
                env.storage().instance().set(&SsKey::ReserveB, &rb);
            }
            pub fn get_reserves(env: Env) -> (i128, i128) {
                let ra = env.storage().instance().get(&SsKey::ReserveA).unwrap_or(0i128);
                let rb = env.storage().instance().get(&SsKey::ReserveB).unwrap_or(0i128);
                (ra, rb)
            }
            pub fn swap(env: Env, a0: i128, a1: i128, to: Address) {
                let a = env.storage().instance().get(&SsKey::TokenA).unwrap();
                let b = env.storage().instance().get(&SsKey::TokenB).unwrap();
                let me = env.current_contract_address();
                if a0 > 0 {
                    token::Client::new(&env, &a).transfer(&me, &to, &a0);
                }
                if a1 > 0 {
                    token::Client::new(&env, &b).transfer(&me, &to, &a1);
                }
            }
        }
    }

    // ── Mock Sushi V3 Pool ──
    mod sushi_mock {
        use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, U256};

        #[contracttype]
        #[derive(Clone)]
        pub struct OracleHints {
            pub checkpoint: u32,
            pub slot: u128,
        }

        #[contract]
        pub struct SushiPool;

        #[contracttype]
        enum SushiKey {
            TokenA,
            TokenB,
        }

        #[contractimpl]
        impl SushiPool {
            pub fn init(env: Env, a: Address, b: Address) {
                env.storage().instance().set(&SushiKey::TokenA, &a);
                env.storage().instance().set(&SushiKey::TokenB, &b);
            }

            pub fn get_oracle_hints(env: Env) -> OracleHints {
                let _ = env;
                OracleHints { checkpoint: 1, slot: 2 }
            }

            pub fn swap(
                env: Env,
                sender: Address,
                recipient: Address,
                zero_for_one: bool,
                amount_specified: i128,
                _sqrt_price_limit_x96: U256,
                _hints: OracleHints,
            ) {
                sender.require_auth();
                assert!(amount_specified > 0);
                let a: Address = env.storage().instance().get(&SushiKey::TokenA).unwrap();
                let b: Address = env.storage().instance().get(&SushiKey::TokenB).unwrap();
                let (token_in, token_out) = if zero_for_one { (&a, &b) } else { (&b, &a) };
                let me = env.current_contract_address();
                token::Client::new(&env, token_in).transfer(&sender, &me, &amount_specified);
                token::Client::new(&env, token_out).transfer(&me, &recipient, &amount_specified);
            }
        }
    }

    // ── Mock Comet Pool ──
    mod comet_mock {
        use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

        #[contract]
        pub struct CometPool;

        #[contracttype]
        enum CometKey {
            TokenA,
            TokenB,
        }

        #[contractimpl]
        impl CometPool {
            pub fn init(env: Env, a: Address, b: Address) {
                env.storage().instance().set(&CometKey::TokenA, &a);
                env.storage().instance().set(&CometKey::TokenB, &b);
            }

            pub fn swap_exact_amount_in(
                env: Env,
                token_in: Address,
                token_amount_in: i128,
                token_out: Address,
                min_amount_out: i128,
                _max_price: i128,
                user: Address,
            ) -> (i128, i128) {
                assert!(token_amount_in > 0);
                let me = env.current_contract_address();
                // Mainnet Comet pull_underlying: approve(user, pool) then transfer_from.
                let seq = env.ledger().sequence();
                let approval_ledger = (seq / 100_000 + 1) * 100_000;
                let token_in_client = token::Client::new(&env, &token_in);
                token_in_client.approve(&user, &me, &token_amount_in, &approval_ledger);
                token_in_client.transfer_from(&me, &user, &me, &token_amount_in);
                token::Client::new(&env, &token_out).transfer(&me, &user, &token_amount_in);
                assert!(token_amount_in >= min_amount_out);
                (token_amount_in, 0)
            }
        }
    }

    use {aq_mock::AqPoolClient, comet_mock::CometPoolClient, ss_mock::SsPairClient, sushi_mock::SushiPoolClient};

    // ═══════════════════════════════════════════════════════════════════════
    //  Aquarius tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_aquarius_a2b_true() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, aq_mock::AqPool);
        let p = pid.clone();
        AqPoolClient::new(&env, &pid).init(&a, &b);
        sac_b.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Aquarius,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let before = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &b, 5000, vec![&env, step], 1);
        assert_eq!(out, 5000);
        assert_eq!(tok_b.balance(&user) - before, 5000);
    }

    #[test]
    fn test_aquarius_a2b_false() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, tok_a) = create_token(&env);
        let (b, sac_b, _) = create_token(&env);
        sac_b.mint(&user, &1_000_000);

        let pid = env.register_contract(None, aq_mock::AqPool);
        let p = pid.clone();
        AqPoolClient::new(&env, &pid).init(&a, &b);
        sac_a.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Aquarius,
            token_in: b.clone(),
            token_out: a.clone(),
            in_idx: 1,
            out_idx: 0,
        };
        let before = tok_a.balance(&user);
        let out = single_swap(&env, &agg, &user, &b, &a, 3000, vec![&env, step], 1);
        assert_eq!(out, 3000);
        assert_eq!(tok_a.balance(&user) - before, 3000);
    }

    #[test]
    fn test_aquarius_rejects_low_output() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, _) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, aq_mock::AqPool);
        let p = pid.clone();
        AqPoolClient::new(&env, &pid).init(&a, &b);
        sac_b.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Aquarius,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            single_swap(&env, &agg, &user, &a, &b, 5000, vec![&env, step], 9999);
        }))
        .is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Soroswap tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_soroswap_a2b_true() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, ss_mock::SsPair);
        let p = pid.clone();
        SsPairClient::new(&env, &pid).init(&a, &b, &100_000, &100_000);
        sac_a.mint(&p, &100_000);
        sac_b.mint(&p, &100_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::SoroswapPair,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let before = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &b, 1000, vec![&env, step], 1);
        assert_eq!(out, 987);
        assert_eq!(tok_b.balance(&user) - before, 987);
    }

    #[test]
    fn test_soroswap_a2b_false() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, tok_a) = create_token(&env);
        let (b, sac_b, _) = create_token(&env);
        sac_b.mint(&user, &1_000_000);

        let pid = env.register_contract(None, ss_mock::SsPair);
        let p = pid.clone();
        SsPairClient::new(&env, &pid).init(&a, &b, &100_000, &100_000);
        sac_a.mint(&p, &100_000);
        sac_b.mint(&p, &100_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::SoroswapPair,
            token_in: b.clone(),
            token_out: a.clone(),
            in_idx: 1,
            out_idx: 0,
        };
        let before = tok_a.balance(&user);
        let out = single_swap(&env, &agg, &user, &b, &a, 1000, vec![&env, step], 1);
        assert_eq!(out, 987);
        assert_eq!(tok_a.balance(&user) - before, 987);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Sushi tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_sushi_swap_with_hints() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, sushi_mock::SushiPool);
        let p = pid.clone();
        SushiPoolClient::new(&env, &pid).init(&a, &b);
        sac_b.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Sushi,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let before = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &b, 4000, vec![&env, step], 1);
        assert_eq!(out, 4000);
        assert_eq!(tok_b.balance(&user) - before, 4000);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Comet tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_comet_swap_exact_amount_in() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, comet_mock::CometPool);
        let p = pid.clone();
        CometPoolClient::new(&env, &pid).init(&a, &b);
        sac_b.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::CometDex,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let before = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &b, 2500, vec![&env, step], 1);
        assert_eq!(out, 2500);
        assert_eq!(tok_b.balance(&user) - before, 2500);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Multi-hop
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_multi_hop() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        let (c, sac_c, tok_c) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let aq_id = env.register_contract(None, aq_mock::AqPool);
        let aq = aq_id.clone();
        AqPoolClient::new(&env, &aq_id).init(&a, &b);
        sac_b.mint(&aq, &10_000_000);

        let ss_id = env.register_contract(None, ss_mock::SsPair);
        let ss = ss_id.clone();
        SsPairClient::new(&env, &ss_id).init(&b, &c, &100_000, &100_000);
        sac_b.mint(&ss, &100_000);
        sac_c.mint(&ss, &100_000);

        let steps = vec![
            &env,
            SwapStep {
                dex_id: aq,
                dex_type: DexType::Aquarius,
                token_in: a.clone(),
                token_out: b.clone(),
                in_idx: 0,
                out_idx: 1,
            },
            SwapStep {
                dex_id: ss,
                dex_type: DexType::SoroswapPair,
                token_in: b,
                token_out: c.clone(),
                in_idx: 0,
                out_idx: 1,
            },
        ];
        let before_c = tok_c.balance(&user);
        let before_b = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &c, 5000, steps, 1);
        assert_eq!(out, 4748);
        assert_eq!(tok_c.balance(&user) - before_c, 4748);
        assert_eq!(tok_b.balance(&user) - before_b, 0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Split swap
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_swap_split_two_routes() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let aq_id = env.register_contract(None, aq_mock::AqPool);
        let aq = aq_id.clone();
        AqPoolClient::new(&env, &aq_id).init(&a, &b);
        sac_b.mint(&aq, &10_000_000);

        let ss_id = env.register_contract(None, ss_mock::SsPair);
        let ss = ss_id.clone();
        SsPairClient::new(&env, &ss_id).init(&a, &b, &100_000, &100_000);
        sac_b.mint(&ss, &100_000);
        sac_a.mint(&ss, &100_000);

        let sub_routes = vec![
            &env,
            SubRoute {
                amount_in: 3000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: aq,
                        dex_type: DexType::Aquarius,
                        token_in: a.clone(),
                        token_out: b.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
            SubRoute {
                amount_in: 2000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: ss,
                        dex_type: DexType::SoroswapPair,
                        token_in: a.clone(),
                        token_out: b.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];
        let before = tok_b.balance(&user);
        let total = agg.swap(&user, &a, &b, &sub_routes, &1);
        assert_eq!(total, 4955);
        assert_eq!(tok_b.balance(&user) - before, 4955);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Round-trip swap (base → bridge → base)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_round_trip_swap_two_legs() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (base, sac_base, tok_base) = create_token(&env);
        let (bridge, sac_bridge, _) = create_token(&env);
        sac_base.mint(&user, &1_000_000);

        let out_pid = env.register_contract(None, aq_mock::AqPool);
        let out_pool = out_pid.clone();
        AqPoolClient::new(&env, &out_pid).init(&base, &bridge);
        sac_bridge.mint(&out_pool, &10_000_000);

        let back_pid = env.register_contract(None, aq_mock::AqPool);
        let back_pool = back_pid.clone();
        AqPoolClient::new(&env, &back_pid).init(&bridge, &base);
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

        let before = tok_base.balance(&user);
        let out = agg.round_trip_swap(&user, &base, &bridge, &5000, &leg_out, &leg_back, &5000);
        assert_eq!(out, 5000);
        assert_eq!(tok_base.balance(&user) - before, 0);
    }

    #[test]
    fn test_round_trip_swap_split_out_leg() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (base, sac_base, tok_base) = create_token(&env);
        let (bridge, sac_bridge, _) = create_token(&env);
        sac_base.mint(&user, &1_000_000);

        let aq1_id = env.register_contract(None, aq_mock::AqPool);
        let aq1 = aq1_id.clone();
        AqPoolClient::new(&env, &aq1_id).init(&base, &bridge);
        sac_bridge.mint(&aq1, &10_000_000);

        let aq2_id = env.register_contract(None, aq_mock::AqPool);
        let aq2 = aq2_id.clone();
        AqPoolClient::new(&env, &aq2_id).init(&base, &bridge);
        sac_bridge.mint(&aq2, &10_000_000);

        let back_pid = env.register_contract(None, aq_mock::AqPool);
        let back_pool = back_pid.clone();
        AqPoolClient::new(&env, &back_pid).init(&bridge, &base);
        sac_base.mint(&back_pool, &20_000);

        let leg_out = vec![
            &env,
            SubRoute {
                amount_in: 3000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: aq1,
                        dex_type: DexType::Aquarius,
                        token_in: base.clone(),
                        token_out: bridge.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
            SubRoute {
                amount_in: 2000,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: aq2,
                        dex_type: DexType::Aquarius,
                        token_in: base.clone(),
                        token_out: bridge.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];
        let bridge_amt = 5000_i128;
        let leg_back = vec![
            &env,
            SubRoute {
                amount_in: bridge_amt,
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

        let before = tok_base.balance(&user);
        let out = agg.round_trip_swap(&user, &base, &bridge, &5000, &leg_out, &leg_back, &5000);
        assert_eq!(out, 5000);
        assert_eq!(tok_base.balance(&user) - before, 0);
    }

    /// Single leg_back: `amount_in` need not equal on-chain bridge — weight
    /// becomes the entire `o1`.
    #[test]
    fn test_round_trip_leg_back_weight_mismatch_single() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (base, sac_base, tok_base) = create_token(&env);
        let (bridge, sac_bridge, _) = create_token(&env);
        sac_base.mint(&user, &1_000_000);

        let out_pid = env.register_contract(None, aq_mock::AqPool);
        let out_pool = out_pid.clone();
        AqPoolClient::new(&env, &out_pid).init(&base, &bridge);
        sac_bridge.mint(&out_pool, &10_000_000);

        let back_pid = env.register_contract(None, aq_mock::AqPool);
        let back_pool = back_pid.clone();
        AqPoolClient::new(&env, &back_pid).init(&bridge, &base);
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
        // Quoted bridge 9100; mock leg_out yields 5000. Rescale → 5000.
        let leg_back = vec![
            &env,
            SubRoute {
                amount_in: 9100,
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

        let before = tok_base.balance(&user);
        let out = agg.round_trip_swap(&user, &base, &bridge, &5000, &leg_out, &leg_back, &5000);
        assert_eq!(out, 5000);
        assert_eq!(tok_base.balance(&user) - before, 0);
    }

    /// Split leg_back: weights 600:310 of quoted USDC, rescaled to actual o1.
    #[test]
    fn test_round_trip_leg_back_split_rescale() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (base, sac_base, tok_base) = create_token(&env);
        let (bridge, sac_bridge, _) = create_token(&env);
        sac_base.mint(&user, &1_000_000);

        let out_pid = env.register_contract(None, aq_mock::AqPool);
        let out_pool = out_pid.clone();
        AqPoolClient::new(&env, &out_pid).init(&base, &bridge);
        sac_bridge.mint(&out_pool, &10_000_000);

        let back1_id = env.register_contract(None, aq_mock::AqPool);
        let back1 = back1_id.clone();
        AqPoolClient::new(&env, &back1_id).init(&bridge, &base);
        sac_base.mint(&back1, &10_000_000);

        let back2_id = env.register_contract(None, aq_mock::AqPool);
        let back2 = back2_id.clone();
        AqPoolClient::new(&env, &back2_id).init(&bridge, &base);
        sac_base.mint(&back2, &10_000_000);

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
                amount_in: 600,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: back1,
                        dex_type: DexType::Aquarius,
                        token_in: bridge.clone(),
                        token_out: base.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
            SubRoute {
                amount_in: 310,
                steps: vec![
                    &env,
                    SwapStep {
                        dex_id: back2,
                        dex_type: DexType::Aquarius,
                        token_in: bridge.clone(),
                        token_out: base.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ];

        let before = tok_base.balance(&user);
        let out = agg.round_trip_swap(&user, &base, &bridge, &5000, &leg_out, &leg_back, &5000);
        assert_eq!(out, 5000);
        assert_eq!(tok_base.balance(&user) - before, 0);
    }

    #[test]
    fn test_scale_sub_routes_remainder() {
        let env = Env::default();
        let dummy = gen_addr(&env);
        let step = SwapStep {
            dex_id: dummy.clone(),
            dex_type: DexType::Aquarius,
            token_in: dummy.clone(),
            token_out: dummy.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let routes = vec![
            &env,
            SubRoute {
                amount_in: 600,
                steps: vec![&env, step.clone()],
            },
            SubRoute {
                amount_in: 310,
                steps: vec![&env, step],
            },
        ];
        let scaled = math::scale_sub_routes_to_total(&env, &routes, 5000);
        assert_eq!(scaled.len(), 2);
        assert_eq!(scaled.get(0).unwrap().amount_in, 3296); // 5000 * 600 / 910
        assert_eq!(scaled.get(1).unwrap().amount_in, 1704); // remainder
        assert_eq!(
            scaled.get(0).unwrap().amount_in + scaled.get(1).unwrap().amount_in,
            5000
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Auth (real, no mock)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_aquarius_exact_match() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        let (b, sac_b, tok_b) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let pid = env.register_contract(None, aq_mock::AqPool);
        let p = pid.clone();
        AqPoolClient::new(&env, &pid).init(&a, &b);
        sac_b.mint(&p, &10_000_000);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Aquarius,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        let before = tok_b.balance(&user);
        let out = single_swap(&env, &agg, &user, &a, &b, 1000, vec![&env, step], 1000);
        assert_eq!(out, 1000);
        assert_eq!(tok_b.balance(&user) - before, 1000);
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  Edge cases
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic(expected = "Empty steps")]
    fn test_empty_steps_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);
        let (token_in, sac, _) = create_token(&env);
        let (token_out, _, _) = create_token(&env);
        sac.mint(&user, &1_000_000);
        let sub = SubRoute {
            amount_in: 1000,
            steps: vec![&env],
        };
        agg.swap(&user, &token_in, &token_out, &vec![&env, sub], &1);
    }

    #[test]
    #[should_panic(expected = "Disconnected sub-route")]
    fn test_disconnected_sub_route_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);
        let (a, sac_a, _) = create_token(&env);
        let (b, _, _) = create_token(&env);
        let (c, _, _) = create_token(&env);
        let (d, _, _) = create_token(&env);
        sac_a.mint(&user, &1_000_000);

        let sub = SubRoute {
            amount_in: 1000,
            steps: vec![
                &env,
                SwapStep {
                    dex_id: gen_addr(&env),
                    dex_type: DexType::Aquarius,
                    token_in: a.clone(),
                    token_out: b,
                    in_idx: 0,
                    out_idx: 1,
                },
                SwapStep {
                    dex_id: gen_addr(&env),
                    dex_type: DexType::Aquarius,
                    token_in: c,
                    token_out: d.clone(),
                    in_idx: 0,
                    out_idx: 1,
                },
            ],
        };
        agg.swap(&user, &a, &d, &vec![&env, sub], &1);
    }

    #[test]
    #[should_panic(expected = "not sufficient")]
    fn test_insufficient_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let user = gen_addr(&env);
        let (_, agg) = setup_agg(&env);

        let (a, sac_a, _) = create_token(&env);
        sac_a.mint(&user, &100);
        let (b, _, _) = create_token(&env);

        let pid = env.register_contract(None, aq_mock::AqPool);
        let p = pid.clone();
        AqPoolClient::new(&env, &pid).init(&a, &b);

        let step = SwapStep {
            dex_id: p,
            dex_type: DexType::Aquarius,
            token_in: a.clone(),
            token_out: b.clone(),
            in_idx: 0,
            out_idx: 1,
        };
        single_swap(&env, &agg, &user, &a, &b, 1000, vec![&env, step], 1);
    }
}

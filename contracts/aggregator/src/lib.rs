#![no_std]
//! Stellar DEX Aggregator Contract
//!
//! Executes multi-hop and split-order swaps atomically across Soroban DEXes
//! (Aquarius, Soroswap, Phoenix).
//!
//! Two main entry points:
//! - `swap()`: Single-path multi-hop swap (A→B→C through different DEXes)
//! - `split_swap()`: Split-order execution (0.3A via path1, 0.7A via path2)
//!
//! Key design: the contract holds no funds permanently.
//! Users approve token transfers, the contract executes swaps, and outputs go
//! directly back to the user — all in one atomic invocation.

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol, Vec, Val,
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    IntoVal,
};

/// Supported DEX protocol types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DexType {
    Aquarius,
    SoroswapPair,
    Phoenix,
}

/// A single swap step in the aggregation path
#[contracttype]
#[derive(Clone, Debug)]
pub struct SwapStep {
    /// DEX pool contract address
    pub dex_id: Address,
    /// Which DEX protocol
    pub dex_type: DexType,
    /// Input token for this step
    pub token_in: Address,
    /// Output token for this step
    pub token_out: Address,
    /// Direction: true = token_a -> token_b, false = token_b -> token_a
    pub a2b: bool,
}

/// A sub-route in a split order.
/// Each sub-route has its own amount and path (sequence of swap steps).
#[contracttype]
#[derive(Clone, Debug)]
pub struct SubRoute {
    /// Amount of input token allocated to this sub-route
    pub amount_in: i128,
    /// Swap steps for this sub-route (multi-hop path)
    pub steps: Vec<SwapStep>,
}

#[contract]
pub struct AggregatorContract;

#[contractimpl]
impl AggregatorContract {
    /// Execute a single-path multi-hop swap atomically.
    ///
    /// Use this when the optimal route is a single path (no splitting needed).
    ///
    /// Flow:
    /// 1. Pull `amount_in` of `token_in` from user
    /// 2. Execute each swap step sequentially (A→B→C)
    /// 3. Verify final output >= `min_amount_out`
    /// 4. Transfer output to user
    pub fn swap(
        env: Env,
        user: Address,
        token_in: Address,
        amount_in: i128,
        steps: Vec<SwapStep>,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();

        let contract_addr = env.current_contract_address();

        // Pull input tokens from user
        let token_in_client = token::Client::new(&env, &token_in);
        token_in_client.transfer(&user, &contract_addr, &amount_in);

        // Execute swap chain
        let output = Self::execute_path(&env, &steps, amount_in, &contract_addr);

        // Verify minimum output
        assert!(output >= min_amount_out, "Output below minimum");

        // Determine output token (last step's token_out)
        let last_step = steps.last().expect("Empty steps");
        let token_out_client = token::Client::new(&env, &last_step.token_out);
        token_out_client.transfer(&contract_addr, &user, &output);

        output
    }

    /// Execute a split-order swap atomically.
    ///
    /// Splits the input across multiple paths for better execution:
    /// e.g., 30% via Soroswap (A→B), 70% via Aquarius (A→C→B)
    ///
    /// Flow:
    /// 1. Pull total `amount_in` of `token_in` from user
    /// 2. For each sub-route: execute its path with its allocated amount
    /// 3. Sum all outputs (must all produce the same `token_out`)
    /// 4. Verify total output >= `min_amount_out`
    /// 5. Transfer total output to user
    ///
    /// All sub-routes MUST produce the same output token.
    pub fn split_swap(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();

        let contract_addr = env.current_contract_address();

        // Calculate total input
        let mut total_in: i128 = 0;
        for sr in sub_routes.iter() {
            total_in += sr.amount_in;
        }
        assert!(total_in > 0, "Total input must be positive");

        // Pull total input from user
        let token_in_client = token::Client::new(&env, &token_in);
        token_in_client.transfer(&user, &contract_addr, &total_in);

        // Execute each sub-route and accumulate output
        let mut total_output: i128 = 0;

        for sr in sub_routes.iter() {
            // Verify first step starts with token_in
            if let Some(first_step) = sr.steps.first() {
                assert!(first_step.token_in == token_in, "Sub-route must start with token_in");
            }
            // Verify last step ends with token_out
            if let Some(last_step) = sr.steps.last() {
                assert!(last_step.token_out == token_out, "Sub-route must end with token_out");
            }

            let output = Self::execute_path(&env, &sr.steps, sr.amount_in, &contract_addr);
            total_output += output;
        }

        // Verify minimum output
        assert!(total_output >= min_amount_out, "Split output below minimum");

        // Transfer total output to user
        let token_out_client = token::Client::new(&env, &token_out);
        token_out_client.transfer(&contract_addr, &user, &total_output);

        total_output
    }

    /// Execute a path (sequence of swap steps) and return the final output amount.
    fn execute_path(env: &Env, steps: &Vec<SwapStep>, amount_in: i128, my_address: &Address) -> i128 {
        let mut current_amount = amount_in;

        for step in steps.iter() {
            current_amount = Self::execute_step(env, &step, current_amount, my_address);
        }

        current_amount
    }

    /// Execute a single swap step on the appropriate DEX.
    fn execute_step(env: &Env, step: &SwapStep, amount_in: i128, my_address: &Address) -> i128 {
        match step.dex_type {
            DexType::Aquarius => {
                // Aquarius: swap(user, in_idx, out_idx, in_amount, out_min) -> u128
                let (in_idx, out_idx): (u32, u32) = if step.a2b { (0, 1) } else { (1, 0) };

                // Pre-authorize token transfer (Aquarius pulls from user internally)
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
                    })
                ]);

                let args = soroban_sdk::vec![
                    env,
                    my_address.into_val(env),
                    in_idx.into_val(env),
                    out_idx.into_val(env),
                    (amount_in as u128).into_val(env),
                    0u128.into_val(env), // min_out = 0, we check at the end
                ];

                let received: u128 = env.invoke_contract(
                    &step.dex_id,
                    &Symbol::new(env, "swap"),
                    args,
                );
                received as i128
            }

            DexType::SoroswapPair => {
                // Soroswap flash-swap: transfer to pair, then call swap()
                let reserves: (i128, i128) = env.invoke_contract(
                    &step.dex_id,
                    &Symbol::new(env, "get_reserves"),
                    soroban_sdk::vec![env],
                );

                let (reserve_in, reserve_out) = if step.a2b {
                    (reserves.0, reserves.1)
                } else {
                    (reserves.1, reserves.0)
                };

                // Compute expected output (matching on-chain formula)
                let amount_in_u = amount_in as u128;
                let fee = (amount_in_u * 3 + 999) / 1000;
                let in_net = amount_in_u - fee;
                let expected_out = if reserve_in > 0 && reserve_out > 0 {
                    (in_net * (reserve_out as u128)) / ((reserve_in as u128) + in_net)
                } else {
                    0
                };

                // Transfer token_in to pair
                let token_client = token::Client::new(env, &step.token_in);
                token_client.transfer(my_address, &step.dex_id, &amount_in);

                // Call swap
                let (amount0_out, amount1_out): (i128, i128) = if step.a2b {
                    (0, expected_out as i128)
                } else {
                    (expected_out as i128, 0)
                };

                let args = soroban_sdk::vec![
                    env,
                    amount0_out.into_val(env),
                    amount1_out.into_val(env),
                    my_address.into_val(env),
                ];
                let _: Val = env.invoke_contract(&step.dex_id, &Symbol::new(env, "swap"), args);
                expected_out as i128
            }

            DexType::Phoenix => {
                // Phoenix: swap(sender, offer_asset, offer_amount, ...)
                // Fee on output, need balance diff to determine actual output
                let token_out_client = token::Client::new(env, &step.token_out);
                let balance_before = token_out_client.balance(my_address);

                // Pre-authorize transfer
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
                    })
                ]);

                let none_val: Val = ().into_val(env);
                let args = soroban_sdk::vec![
                    env,
                    my_address.into_val(env),
                    step.token_in.into_val(env),
                    amount_in.into_val(env),
                    none_val.clone(),
                    none_val.clone(),
                    none_val.clone(),
                    none_val,
                ];
                let _: Val = env.invoke_contract(&step.dex_id, &Symbol::new(env, "swap"), args);

                let balance_after = token_out_client.balance(my_address);
                balance_after - balance_before
            }
        }
    }
}

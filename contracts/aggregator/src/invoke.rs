use {
    crate::{errors::AggregatorError, math},
    lumagg_contract_types::{DexType, SwapStep},
    soroban_sdk::{
        auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
        token, Address, Env, IntoVal, Symbol, Val, Vec,
    },
};

/// Comet rounds the approval ledger to avoid simulation vs execution
/// sequence mismatch.
pub(crate) fn comet_approval_ledger(env: &Env) -> u32 {
    let seq = env.ledger().sequence();
    (seq / 100_000 + 1) * 100_000
}

/// Execute a path (sequence of swap steps) and return the final output
/// amount.
pub(crate) fn execute_path(
    env: &Env,
    steps: &Vec<SwapStep>,
    amount_in: i128,
    my_address: &Address,
    path_base: u32,
    max_depth: &mut u32,
) -> i128 {
    soroban_sdk::assert_with_error!(env, amount_in > 0, AggregatorError::InvalidAmount);
    let mut current_amount = amount_in;

    for (i, step) in steps.iter().enumerate() {
        soroban_sdk::assert_with_error!(env, step.token_in != step.token_out, AggregatorError::InvalidStep);
        soroban_sdk::assert_with_error!(env, step.in_idx != step.out_idx, AggregatorError::InvalidStep);
        let hop_idx = path_base + i as u32;
        current_amount = execute_step(env, &step, current_amount, my_address, hop_idx);
        soroban_sdk::assert_with_error!(env, current_amount > 0, AggregatorError::ZeroStepOutput);
        let depth = (i as u32) + 1;
        if depth > *max_depth {
            *max_depth = depth;
        }
    }

    current_amount
}

pub(crate) fn dex_tag(dex_type: &DexType) -> u32 {
    match dex_type {
        DexType::Aquarius => 0,
        DexType::SoroswapPair => 1,
        DexType::Phoenix => 2,
        DexType::Sushi => 3,
        DexType::CometDex => 4,
    }
}

/// Execute a single swap step on the appropriate DEX.
pub(crate) fn execute_step(env: &Env, step: &SwapStep, amount_in: i128, my_address: &Address, hop_idx: u32) -> i128 {
    let output = execute_step_inner(env, step, amount_in, my_address);
    env.events().publish(
        (Symbol::new(env, "leg"),),
        (
            hop_idx,
            dex_tag(&step.dex_type),
            step.dex_id.clone(),
            step.token_in.clone(),
            amount_in,
        ),
    );
    output
}

pub(crate) fn execute_step_inner(env: &Env, step: &SwapStep, amount_in: i128, my_address: &Address) -> i128 {
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
                            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xfd, 0x89,
                            0x63, 0xef, 0xd1, 0xfc, 0x6a, 0x50, 0x64, 0x88, 0x49, 0x5d, 0x95, 0x1d, 0x52, 0x63, 0x98,
                            0x8d, 0x25,
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
            let approval_ledger = comet_approval_ledger(env);

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

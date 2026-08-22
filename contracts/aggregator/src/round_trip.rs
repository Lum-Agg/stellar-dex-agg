use {
    crate::{errors::AggregatorError, events, math, validate},
    lumagg_contract_types::SubRoute,
    soroban_sdk::{token, Address, Env, Vec},
};

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
///   `SubRoute.amount_in` is an absolute base-token input; they **must** sum to
///   `amount_in`.
/// - `leg_back`: sub-routes from `bridge_token` to `base_token`. Each
///   `SubRoute.amount_in` is a **positive weight** (quoted bridge amounts are
///   fine). After `leg_out` produces actual bridge total `o1`, weights are
///   rescaled so executed inputs sum **exactly** to `o1` (last sub-route
///   receives the remainder). Callers do **not** need to know `o1` at submit
///   time.
/// - `min_amount_out`: minimum total `base_token` returned (principal + profit
///   floor)
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
    soroban_sdk::assert_with_error!(env, amount_in > 0, AggregatorError::InvalidAmount);
    soroban_sdk::assert_with_error!(env, min_amount_out >= amount_in, AggregatorError::InvalidMinimumOut);
    soroban_sdk::assert_with_error!(env, base_token != bridge_token, AggregatorError::InvalidRoute);

    let contract_addr = env.current_contract_address();

    let mut leg_counter: u32 = 0;

    let leg_out_in = validate::validate_sub_routes(&env, &base_token, &bridge_token, &leg_out);
    validate::validate_sub_routes(&env, &bridge_token, &base_token, &leg_back);
    soroban_sdk::assert_with_error!(env, leg_out_in == amount_in, AggregatorError::InvalidAmount);
    let is_split = leg_out.len() > 1 || leg_back.len() > 1;

    // Pull base from user
    let base_client = token::Client::new(&env, &base_token);
    base_client.transfer(&user, &contract_addr, &amount_in);

    let bridge_total = crate::swap::execute_sub_routes(&env, &leg_out, &contract_addr, &mut leg_counter);
    soroban_sdk::assert_with_error!(env, bridge_total > 0, AggregatorError::ZeroStepOutput);

    // Scale leg_back weights → absolute bridge inputs that sum to o1.
    let scaled_back = math::scale_sub_routes_to_total(&env, &leg_back, bridge_total);

    let base_total = crate::swap::execute_sub_routes(&env, &scaled_back, &contract_addr, &mut leg_counter);

    soroban_sdk::assert_with_error!(env, base_total >= min_amount_out, AggregatorError::OutputBelowMinimum);

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

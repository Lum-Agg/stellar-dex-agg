use crate::{events, validate};
use lumagg_contract_types::SubRoute;
use soroban_sdk::{token, Address, Env, Vec};

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
    let total_output = execute_sub_routes(&env, &sub_routes, &contract_addr, &mut leg_counter);

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


/// Execute sub-routes that share the same token_in → token_out pair;
/// returns total output.
///
/// Parallel split paths share hop indices (`path_base + hop`). After all
/// paths run, `leg_counter` advances by the **serial depth** (longest path
/// hop count), not by total hop executions. Exact routed volume is derived
/// from each emitted `leg` event's actual input.
pub(crate) fn execute_sub_routes(
    env: &Env,
    sub_routes: &Vec<SubRoute>,
    contract_addr: &Address,
    leg_counter: &mut u32,
) -> i128 {
    let path_base = *leg_counter;
    let mut max_depth: u32 = 0;
    let mut total_output: i128 = 0;
    for sr in sub_routes.iter() {
        let output = crate::invoke::execute_path(env, &sr.steps, sr.amount_in, contract_addr, path_base, &mut max_depth);
        total_output = total_output.checked_add(output).expect("total output overflow");
    }
    *leg_counter = path_base.checked_add(max_depth).expect("leg counter overflow");
    total_output
}


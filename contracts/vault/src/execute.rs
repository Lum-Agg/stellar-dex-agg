use {
    crate::{auth, types::AggregatorContractClient},
    lumagg_contract_types::SubRoute,
    soroban_sdk::{token, Address, Env, Vec},
};

pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
    from.require_auth();
    assert!(amount > 0, "amount must be positive");
    let vault = env.current_contract_address();
    token::Client::new(&env, &token).transfer(&from, &vault, &amount);
}

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
    auth::require_caller(&env, &caller);
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

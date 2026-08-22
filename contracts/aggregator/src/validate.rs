use {
    crate::{admin, errors::AggregatorError},
    lumagg_contract_types::SubRoute,
    soroban_sdk::{Address, Env, Vec},
};

pub(crate) fn validate_sub_routes(
    env: &Env,
    token_in: &Address,
    token_out: &Address,
    sub_routes: &Vec<SubRoute>,
) -> i128 {
    soroban_sdk::assert_with_error!(env, !sub_routes.is_empty(), AggregatorError::EmptyRoutes);

    let mut total_in = 0i128;
    for sr in sub_routes.iter() {
        soroban_sdk::assert_with_error!(env, sr.amount_in > 0, AggregatorError::InvalidAmount);
        total_in = total_in
            .checked_add(sr.amount_in)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, AggregatorError::ArithmeticOverflow));
        soroban_sdk::assert_with_error!(env, !sr.steps.is_empty(), AggregatorError::InvalidRoute);

        let first_step = sr.steps.first().unwrap();
        let last_step = sr.steps.last().unwrap();
        soroban_sdk::assert_with_error!(env, first_step.token_in == *token_in, AggregatorError::InvalidRoute);
        soroban_sdk::assert_with_error!(env, last_step.token_out == *token_out, AggregatorError::InvalidRoute);

        for i in 1..sr.steps.len() {
            let previous = sr.steps.get(i - 1).unwrap();
            let current = sr.steps.get(i).unwrap();
            soroban_sdk::assert_with_error!(
                env,
                previous.token_out == current.token_in,
                AggregatorError::DisconnectedRoute
            );
        }
    }
    total_in
}

pub(crate) fn validate_registered_venues(env: &soroban_sdk::Env, sub_routes: &Vec<SubRoute>) {
    for route in sub_routes.iter() {
        for step in route.steps.iter() {
            soroban_sdk::assert_with_error!(
                env,
                admin::is_venue(env.clone(), step.dex_type.clone(), step.dex_id.clone()),
                AggregatorError::VenueNotRegistered
            );
        }
    }
}

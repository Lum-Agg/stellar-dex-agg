use {
    crate::admin,
    lumagg_contract_types::SubRoute,
    soroban_sdk::{Address, Vec},
};

pub(crate) fn validate_sub_routes(token_in: &Address, token_out: &Address, sub_routes: &Vec<SubRoute>) -> i128 {
    assert!(!sub_routes.is_empty(), "Empty sub_routes");

    let mut total_in = 0i128;
    for sr in sub_routes.iter() {
        assert!(sr.amount_in > 0, "Sub-route amount must be positive");
        total_in = total_in.checked_add(sr.amount_in).expect("total input overflow");
        assert!(!sr.steps.is_empty(), "Empty steps");

        let first_step = sr.steps.first().unwrap();
        let last_step = sr.steps.last().unwrap();
        assert!(first_step.token_in == *token_in, "Sub-route must start with token_in");
        assert!(last_step.token_out == *token_out, "Sub-route must end with token_out");

        for i in 1..sr.steps.len() {
            let previous = sr.steps.get(i - 1).unwrap();
            let current = sr.steps.get(i).unwrap();
            assert!(previous.token_out == current.token_in, "Disconnected sub-route");
        }
    }
    total_in
}

pub(crate) fn validate_registered_venues(env: &soroban_sdk::Env, sub_routes: &Vec<SubRoute>) {
    for route in sub_routes.iter() {
        for step in route.steps.iter() {
            assert!(
                admin::is_venue(env.clone(), step.dex_type.clone(), step.dex_id.clone()),
                "venue is not registered"
            );
        }
    }
}

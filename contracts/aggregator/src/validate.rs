use lumagg_contract_types::SubRoute;
use soroban_sdk::{Address, Vec};

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

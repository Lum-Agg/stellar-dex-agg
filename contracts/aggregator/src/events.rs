use soroban_sdk::{Address, Env, Symbol};

pub(crate) fn publish_swap(
    env: &Env,
    user: &Address,
    token_in: &Address,
    token_out: &Address,
    total_in: i128,
    total_output: i128,
    sub_route_count: u32,
) {
    env.events().publish(
        (Symbol::new(env, "swap"),),
        (
            user.clone(),
            token_in.clone(),
            token_out.clone(),
            total_in,
            total_output,
            sub_route_count,
        ),
    );
}

pub(crate) fn publish_rt(
    env: &Env,
    user: &Address,
    base_token: &Address,
    bridge_token: &Address,
    amount_in: i128,
    base_total: i128,
    leg_counter: u32,
    is_split: bool,
) {
    env.events().publish(
        (Symbol::new(env, "rt"),),
        (
            user.clone(),
            base_token.clone(),
            bridge_token.clone(),
            amount_in,
            base_total,
            leg_counter,
            is_split,
        ),
    );
}

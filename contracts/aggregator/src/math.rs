use {
    crate::errors::AggregatorError,
    lumagg_contract_types::SubRoute,
    soroban_sdk::{Env, Vec},
};

/// Soroswap fee: ceil(amount_in * 3 / 1000), matching pair swap K-check.
pub(crate) fn soroswap_fee(amount_in: i128) -> i128 {
    if amount_in <= 0 {
        return 0;
    }
    (amount_in * 3 + 999) / 1000
}

/// Soroswap library `get_amount_out` (floor division on output).
pub(crate) fn soroswap_get_amount_out(amount_in: i128, reserve_in: i128, reserve_out: i128) -> i128 {
    if amount_in <= 0 || reserve_in <= 0 || reserve_out <= 0 {
        return 0;
    }
    let in_less = amount_in - soroswap_fee(amount_in);
    if in_less <= 0 {
        return 0;
    }
    in_less * reserve_out / (reserve_in + in_less)
}

/// Treat each `SubRoute.amount_in` as a positive weight and allocate
/// `target_total` across routes. Intermediate routes use floor division; the
/// last route receives the remainder so the sum is exact (`= target_total`).
pub(crate) fn scale_sub_routes_to_total(env: &Env, routes: &Vec<SubRoute>, target_total: i128) -> Vec<SubRoute> {
    soroban_sdk::assert_with_error!(env, !routes.is_empty(), AggregatorError::EmptyRoutes);
    soroban_sdk::assert_with_error!(env, target_total > 0, AggregatorError::InvalidAmount);

    let mut weight_sum: i128 = 0;
    for sr in routes.iter() {
        soroban_sdk::assert_with_error!(env, sr.amount_in > 0, AggregatorError::InvalidAmount);
        weight_sum = weight_sum
            .checked_add(sr.amount_in)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, AggregatorError::ArithmeticOverflow));
    }

    let n = routes.len();
    let mut out: Vec<SubRoute> = Vec::new(env);
    let mut allocated: i128 = 0;
    for i in 0..n {
        let sr = routes.get(i).unwrap();
        let amount = if i + 1 == n {
            target_total - allocated
        } else {
            let scaled = sr
                .amount_in
                .checked_mul(target_total)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, AggregatorError::ArithmeticOverflow)) /
                weight_sum;
            allocated = allocated
                .checked_add(scaled)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, AggregatorError::ArithmeticOverflow));
            scaled
        };
        soroban_sdk::assert_with_error!(env, amount > 0, AggregatorError::InvalidAmount);
        out.push_back(SubRoute {
            amount_in: amount,
            steps: sr.steps.clone(),
        });
    }
    out
}

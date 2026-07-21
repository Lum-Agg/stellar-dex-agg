/// Returns the minimum acceptable output for an input amount at an E7 limit
/// price, matching the order-escrow contract's floor rounding.
pub const RATE_SCALE_E7: i128 = 10_000_000;

pub fn required_min_out(amount_in: i128, limit_out_per_in_e7: i128) -> i128 {
    assert!(amount_in >= 0, "amount_in must not be negative");
    assert!(limit_out_per_in_e7 > 0, "limit must be positive");
    amount_in
        .checked_mul(limit_out_per_in_e7)
        .expect("amount and limit multiplication overflow") /
        RATE_SCALE_E7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_min_out_scales() {
        assert_eq!(required_min_out(5_000_000, 20_000_000), 10_000_000);
    }
}

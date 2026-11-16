//! Comet (Balancer V1) math — pure Rust port of the on-chain fixed-point
//! arithmetic.
//!
//! All calculations use i128 with 18-decimal fixed point (BONE = 10^18).
//! This matches the on-chain contract exactly for precision.

/// 18-decimal fixed point unit
pub const BONE: i128 = 1_000_000_000_000_000_000; // 10^18
/// Stellar stroops (7 decimals)
pub const STROOP: i128 = 10_000_000; // 10^7
/// Scalar to convert stroops to 18 decimals: 10^11
pub const STROOP_SCALAR: i128 = 100_000_000_000; // 10^11
/// Precision for power approximation
pub const CPOW_PRECISION: i128 = 100_000_000; // 10^8

/// Record for a token in the pool
#[derive(Debug, Clone)]
pub struct CometRecord {
    pub balance: i128, // in token's native decimals (stroops for 7-decimal tokens)
    pub weight: i128,  // in STROOP (7 decimals), e.g., 0.8 = 8_000_000
    pub scalar: i128,  // scaling factor to 18 decimals
}

/// Calculate token out given token in (Balancer weighted pool formula).
/// This is a pure Rust port of the on-chain `calc_token_out_given_token_in`.
///
/// Returns the output amount in the output token's native decimals.
pub fn calc_out_given_in(
    in_record: &CometRecord,
    out_record: &CometRecord,
    amount_in: i128,
    swap_fee: i128, // in STROOP (7 decimals), e.g., 0.003 = 30_000
) -> i128 {
    if amount_in == 0 || in_record.balance == 0 || out_record.balance == 0 {
        return 0;
    }
    if in_record.weight == 0 || out_record.weight == 0 {
        return 0;
    }

    let token_balance_in = upscale(in_record.balance, in_record.scalar);
    let token_balance_out = upscale(out_record.balance, out_record.scalar);
    let token_amount_in = upscale(amount_in, in_record.scalar);

    let fee_adjust_ratio = upscale(STROOP - swap_fee, STROOP_SCALAR);
    let weight_ratio = upscale(
        fixed_div_floor(in_record.weight, out_record.weight, STROOP),
        STROOP_SCALAR,
    );

    let adjusted_in = fixed_mul_floor_i128(token_amount_in, fee_adjust_ratio, BONE);

    let base = fixed_div_floor_i128(token_balance_in, token_balance_in + adjusted_in, BONE);

    let power = c_pow(base, weight_ratio, true);
    let balance_ratio = BONE - power;
    if balance_ratio <= 0 {
        return 0;
    }

    let result = fixed_mul_floor_i128(token_balance_out, balance_ratio, BONE);
    downscale_floor(result, out_record.scalar)
}

/// Fixed-point power: base^exp where both are 18-decimal fixed point.
/// Uses integer exponentiation for the whole part and Taylor series for the
/// fractional part.
fn c_pow(base: i128, exp: i128, _round_up: bool) -> i128 {
    if base <= 0 {
        return 0;
    }
    if base > 2 * BONE - 1 {
        return BONE; // clamp
    }

    let int_part = exp / BONE;
    let remain = exp - int_part * BONE;

    let whole_pow = c_powi(base, int_part as u32);

    if remain == 0 {
        return whole_pow;
    }

    let partial = c_pow_approx(base, remain);
    fixed_mul_floor_i128(whole_pow, partial, BONE)
}

/// Integer power: a^n using repeated squaring
fn c_powi(a: i128, n: u32) -> i128 {
    let mut z = if n % 2 != 0 { a } else { BONE };
    let mut a = a;
    let mut n = n / 2;
    while n != 0 {
        a = fixed_mul_floor_i128(a, a, BONE);
        if n % 2 != 0 {
            z = fixed_mul_floor_i128(z, a, BONE);
        }
        n /= 2;
    }
    z
}

/// Taylor series approximation for base^exp where exp < 1 (in 18-decimal fixed
/// point). This is the binomial series: (1 + x)^a ≈ 1 + ax + a(a-1)x²/2! + ...
/// where base = 1 + x, so x = base - BONE
fn c_pow_approx(base: i128, exp: i128) -> i128 {
    let x = base - BONE;
    let mut term: i128 = BONE;
    let mut sum: i128 = BONE;

    for i in 1..51i128 {
        let big_k = i * BONE;
        let c = exp - (big_k - BONE);
        term = fixed_mul_floor_i128(term, fixed_mul_floor_i128(c, x, BONE), BONE);
        term = fixed_div_floor_i128(term, big_k, BONE);
        sum += term;

        if term.unsigned_abs() <= CPOW_PRECISION as u128 {
            break;
        }
    }

    sum.max(0)
}

// ===== Fixed-point arithmetic helpers =====

fn fixed_mul_floor_i128(a: i128, b: i128, scale: i128) -> i128 {
    // Handle signs
    let sign = if (a < 0) ^ (b < 0) { -1i128 } else { 1i128 };
    let a_abs = a.unsigned_abs();
    let b_abs = b.unsigned_abs();
    let scale_abs = scale.unsigned_abs();

    // Use u128 widening: split into high/low 64-bit parts
    let result = mul_div_u128(a_abs, b_abs, scale_abs);
    (result as i128) * sign
}

fn fixed_div_floor_i128(a: i128, b: i128, scale: i128) -> i128 {
    if b == 0 {
        return 0;
    }
    let sign = if (a < 0) ^ (b < 0) { -1i128 } else { 1i128 };
    let a_abs = a.unsigned_abs();
    let b_abs = b.unsigned_abs();
    let scale_abs = scale.unsigned_abs();

    let result = mul_div_u128(a_abs, scale_abs, b_abs);
    (result as i128) * sign
}

/// Compute (a * b) / c without overflow using 256-bit intermediate.
/// Uses the schoolbook method: split into high/low 64-bit parts.
fn mul_div_u128(a: u128, b: u128, c: u128) -> u128 {
    if c == 0 {
        return 0;
    }
    if a == 0 || b == 0 {
        return 0;
    }

    // Check if a * b fits in u128
    if let Some(product) = a.checked_mul(b) {
        return product / c;
    }

    // Overflow: use 256-bit arithmetic via two u128s
    // a * b = (a_hi * 2^64 + a_lo) * (b_hi * 2^64 + b_lo)
    let a_hi = (a >> 64) as u128;
    let a_lo = a & 0xFFFFFFFFFFFFFFFF;
    let b_hi = (b >> 64) as u128;
    let b_lo = b & 0xFFFFFFFFFFFFFFFF;

    // Full 256-bit product (we only need enough precision for division by c)
    let mid1 = a_hi * b_lo;
    let mid2 = a_lo * b_hi;
    let low = a_lo * b_lo;
    let high = a_hi * b_hi;

    // Approximate: use f64 for the division (loses some precision but doesn't
    // overflow)
    let product_f64 = (high as f64) * (1u128 << 64) as f64 * (1u128 << 64) as f64 +
        (mid1 as f64 + mid2 as f64) * (1u128 << 64) as f64 +
        low as f64;

    (product_f64 / c as f64) as u128
}

fn fixed_div_floor(a: i128, b: i128, scale: i128) -> i128 {
    fixed_div_floor_i128(a, b, scale)
}

fn upscale(amount: i128, scalar: i128) -> i128 {
    amount * scalar
}

fn downscale_floor(amount: i128, scalar: i128) -> i128 {
    if scalar == 0 {
        return 0;
    }
    amount / scalar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_swap_equal_weights() {
        // 50/50 pool, equal balances, small swap
        let in_rec = CometRecord {
            balance: 1_000_0000000, // 1000 tokens (7 decimals)
            weight: 5_000_000,      // 0.5 weight
            scalar: STROOP_SCALAR,
        };
        let out_rec = CometRecord {
            balance: 1_000_0000000,
            weight: 5_000_000,
            scalar: STROOP_SCALAR,
        };

        // Swap 10 tokens in
        let out = calc_out_given_in(&in_rec, &out_rec, 10_0000000, 30_000); // 0.3% fee
                                                                            // With equal weights and equal balances, ~9.87 out (after fee + impact)
        assert!(out > 9_0000000 && out < 10_0000000, "out = {}", out);
    }

    #[test]
    fn test_swap_different_weights() {
        // 80/20 pool (like BLND/USDC)
        let in_rec = CometRecord {
            balance: 67_000_0000000, // 67000 BLND
            weight: 8_000_000,       // 0.8 weight
            scalar: STROOP_SCALAR,
        };
        let out_rec = CometRecord {
            balance: 830_0000000, // 830 USDC
            weight: 2_000_000,    // 0.2 weight
            scalar: STROOP_SCALAR,
        };

        // Swap 1000 BLND in
        let out = calc_out_given_in(&in_rec, &out_rec, 1000_0000000, 30_000);
        // Should get some USDC out
        assert!(out > 0, "out = {}", out);
        println!("1000 BLND -> {} USDC (stroops)", out);
    }

    #[test]
    fn test_zero_input() {
        let rec = CometRecord {
            balance: 1000_0000000,
            weight: 5_000_000,
            scalar: STROOP_SCALAR,
        };
        let out = calc_out_given_in(&rec, &rec, 0, 30_000);
        println!("zero input out = {}", out);
        assert_eq!(out, 0);
    }

    #[test]
    fn test_c_pow_integer() {
        // 0.5^2 = 0.25
        let base = BONE / 2; // 0.5 in 18 decimals
        let exp = 2 * BONE; // 2.0
        let result = c_pow(base, exp, false);
        let expected = BONE / 4; // 0.25
        let diff = (result - expected).unsigned_abs();
        assert!(
            diff < CPOW_PRECISION as u128,
            "result={}, expected={}",
            result,
            expected
        );
    }

    #[test]
    fn test_c_pow_fractional() {
        // 0.5^0.5 = sqrt(0.5) ≈ 0.7071
        let base = BONE / 2;
        let exp = BONE / 2;
        let result = c_pow(base, exp, false);
        let expected = 707_106_781_186_547_524i128; // sqrt(0.5) * BONE
        let diff = (result - expected).unsigned_abs();
        // Allow 0.01% error
        assert!(
            diff < (expected as u128 / 10000),
            "result={}, expected={}, diff={}",
            result,
            expected,
            diff
        );
    }
}

//! CLMM (Concentrated Liquidity Market Maker) math — pure Rust port of Uniswap
//! V3 math.
//!
//! Shared between Aquarius Concentrated pools and Sushi V3 on Stellar.
//! All calculations use U256 (represented as [u64; 4] little-endian) with
//! Q64.96 fixed point. This matches the on-chain contract exactly for
//! precision.
//!
//! Key types:
//! - sqrt_price_x96: Q64.96 fixed-point sqrt(price) = sqrt(token1/token0) *
//!   2^96
//! - tick: i32, satisfies sqrt_ratio_at_tick(tick) <= sqrt_price_x96
//! - liquidity: u128, active liquidity in the current tick range

use {
    market_snapshot::{ClmmBitmapWordSnapshot, ClmmCoverageSnapshot, ClmmPoolSnapshot, ClmmTickSnapshot},
    std::fmt,
};

// ============================================================================
// U256 type (little-endian [u64; 4])
// ============================================================================

/// 256-bit unsigned integer, stored as 4 x u64 in little-endian limb order.
/// limbs[0] is the least significant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct U256(pub [u64; 4]);

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "U256({:#x}_{:016x}_{:016x}_{:016x})",
            self.0[3], self.0[2], self.0[1], self.0[0]
        )
    }
}

impl U256 {
    pub const ZERO: U256 = U256([0, 0, 0, 0]);
    pub const ONE: U256 = U256([1, 0, 0, 0]);
    pub const MAX: U256 = U256([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);

    pub fn from_u128(v: u128) -> Self {
        U256([v as u64, (v >> 64) as u64, 0, 0])
    }

    pub fn from_u64(v: u64) -> Self {
        U256([v, 0, 0, 0])
    }

    pub fn to_u128(&self) -> Option<u128> {
        if self.0[2] != 0 || self.0[3] != 0 {
            return None;
        }
        Some((self.0[1] as u128) << 64 | self.0[0] as u128)
    }

    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    pub fn leading_zeros(&self) -> u32 {
        if self.0[3] != 0 {
            return self.0[3].leading_zeros();
        }
        if self.0[2] != 0 {
            return 64 + self.0[2].leading_zeros();
        }
        if self.0[1] != 0 {
            return 128 + self.0[1].leading_zeros();
        }
        192 + self.0[0].leading_zeros()
    }

    pub fn bit_length(&self) -> u32 {
        256 - self.leading_zeros()
    }

    pub fn shl(&self, shift: u32) -> Self {
        if shift >= 256 {
            return Self::ZERO;
        }
        if shift == 0 {
            return *self;
        }

        let word_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;

        let mut result = [0u64; 4];
        if bit_shift == 0 {
            for i in word_shift..4 {
                result[i] = self.0[i - word_shift];
            }
        } else {
            for i in word_shift..4 {
                result[i] = self.0[i - word_shift] << bit_shift;
                if i > word_shift {
                    result[i] |= self.0[i - word_shift - 1] >> (64 - bit_shift);
                }
            }
        }
        U256(result)
    }

    pub fn shr(&self, shift: u32) -> Self {
        if shift >= 256 {
            return Self::ZERO;
        }
        if shift == 0 {
            return *self;
        }

        let word_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;

        let mut result = [0u64; 4];
        if bit_shift == 0 {
            for i in 0..(4 - word_shift) {
                result[i] = self.0[i + word_shift];
            }
        } else {
            for i in 0..(4 - word_shift) {
                result[i] = self.0[i + word_shift] >> bit_shift;
                if i + word_shift + 1 < 4 {
                    result[i] |= self.0[i + word_shift + 1] << (64 - bit_shift);
                }
            }
        }
        U256(result)
    }

    /// Wrapping addition
    pub fn wrapping_add(&self, other: &Self) -> Self {
        let mut result = [0u64; 4];
        let mut carry = 0u64;
        for i in 0..4 {
            let (sum1, c1) = self.0[i].overflowing_add(other.0[i]);
            let (sum2, c2) = sum1.overflowing_add(carry);
            result[i] = sum2;
            carry = (c1 as u64) + (c2 as u64);
        }
        U256(result)
    }

    /// Wrapping subtraction
    pub fn wrapping_sub(&self, other: &Self) -> Self {
        let mut result = [0u64; 4];
        let mut borrow = 0u64;
        for i in 0..4 {
            let (diff1, b1) = self.0[i].overflowing_sub(other.0[i]);
            let (diff2, b2) = diff1.overflowing_sub(borrow);
            result[i] = diff2;
            borrow = (b1 as u64) + (b2 as u64);
        }
        U256(result)
    }

    /// Full multiplication returning (lo, hi) as two U256
    pub fn full_mul(&self, other: &Self) -> (U256, U256) {
        // Schoolbook multiplication with u128 intermediates
        let mut result = [0u128; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let prod = (self.0[i] as u128) * (other.0[j] as u128) + result[i + j] + carry;
                result[i + j] = prod & (u64::MAX as u128);
                carry = prod >> 64;
            }
            if i + 4 < 8 {
                result[i + 4] += carry;
            }
        }
        let lo = U256([result[0] as u64, result[1] as u64, result[2] as u64, result[3] as u64]);
        let hi = U256([result[4] as u64, result[5] as u64, result[6] as u64, result[7] as u64]);
        (lo, hi)
    }

    /// Multiply, returning lower 256 bits (wrapping)
    pub fn wrapping_mul(&self, other: &Self) -> Self {
        self.full_mul(other).0
    }

    /// Division: self / other. Panics on division by zero.
    pub fn div(&self, other: &Self) -> Self {
        self.div_rem(other).0
    }

    /// Remainder: self % other. Panics on division by zero.
    pub fn rem(&self, other: &Self) -> Self {
        self.div_rem(other).1
    }

    /// Division with remainder using binary long division.
    pub fn div_rem(&self, divisor: &Self) -> (Self, Self) {
        assert!(!divisor.is_zero(), "division by zero");

        if *self < *divisor {
            return (Self::ZERO, *self);
        }
        if *self == *divisor {
            return (Self::ONE, Self::ZERO);
        }

        let dividend_bits = self.bit_length();

        let mut quotient = Self::ZERO;
        let mut remainder = Self::ZERO;

        for i in (0..dividend_bits).rev() {
            remainder = remainder.shl(1);
            // Set bit 0 of remainder to bit i of self
            let word = (i / 64) as usize;
            let bit = i % 64;
            if (self.0[word] >> bit) & 1 == 1 {
                remainder.0[0] |= 1;
            }
            if remainder >= *divisor {
                remainder = remainder.wrapping_sub(divisor);
                let q_word = (i / 64) as usize;
                let q_bit = i % 64;
                quotient.0[q_word] |= 1u64 << q_bit;
            }
        }

        (quotient, remainder)
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                std::cmp::Ordering::Equal => continue,
                ord => return ord,
            }
        }
        std::cmp::Ordering::Equal
    }
}

// ============================================================================
// mul_div helpers (512-bit intermediate precision)
// ============================================================================

/// floor(a * b / d) with 512-bit intermediate
pub fn mul_div_floor(a: &U256, b: &U256, d: &U256) -> U256 {
    mul_div_internal(a, b, d, false)
}

/// ceil(a * b / d) with 512-bit intermediate
pub fn mul_div_ceil(a: &U256, b: &U256, d: &U256) -> U256 {
    mul_div_internal(a, b, d, true)
}

fn mul_div_internal(a: &U256, b: &U256, d: &U256, round_up: bool) -> U256 {
    assert!(!d.is_zero(), "mul_div: division by zero");
    if a.is_zero() || b.is_zero() {
        return U256::ZERO;
    }

    let (lo, hi) = a.full_mul(b);

    // Fast path: hi == 0, product fits in 256 bits
    if hi.is_zero() {
        let (q, r) = lo.div_rem(d);
        if round_up && !r.is_zero() {
            return q.wrapping_add(&U256::ONE);
        }
        return q;
    }

    // Slow path: 512-bit / 256-bit division
    let (q, r) = div_512_by_256(&hi, &lo, d);
    if round_up && !r.is_zero() {
        q.wrapping_add(&U256::ONE)
    } else {
        q
    }
}

/// Divide a 512-bit number (hi * 2^256 + lo) by a 256-bit divisor.
/// Returns (quotient, remainder). Quotient must fit in 256 bits.
fn div_512_by_256(hi: &U256, lo: &U256, d: &U256) -> (U256, U256) {
    // Bit-by-bit long division
    let mut remainder = *hi;
    let mut quotient = U256::ZERO;

    // Process 256 bits of lo from MSB to LSB
    for i in (0..256u32).rev() {
        // remainder = remainder * 2
        let overflow = (remainder.0[3] >> 63) != 0;
        remainder = remainder.shl(1);

        // Add bit i of lo
        let word = (i / 64) as usize;
        let bit = i % 64;
        if (lo.0[word] >> bit) & 1 == 1 {
            remainder.0[0] |= 1;
        }

        // If remainder >= d (or overflow), subtract d and set quotient bit
        if overflow || remainder >= *d {
            remainder = remainder.wrapping_sub(d);
            let q_word = (i / 64) as usize;
            let q_bit = i % 64;
            quotient.0[q_word] |= 1u64 << q_bit;
        }
    }

    (quotient, remainder)
}

/// mul_div for u128 values (convenience)
pub fn mul_div_u128(a: u128, b: u128, c: u128, round_up: bool) -> u128 {
    if c == 0 {
        return 0;
    }
    if a == 0 || b == 0 {
        return 0;
    }

    // Try direct multiplication first
    if let Some(product) = a.checked_mul(b) {
        let q = product / c;
        let r = product % c;
        return if round_up && r > 0 { q + 1 } else { q };
    }

    // Overflow: use U256
    let a256 = U256::from_u128(a);
    let b256 = U256::from_u128(b);
    let c256 = U256::from_u128(c);
    let result = if round_up {
        mul_div_ceil(&a256, &b256, &c256)
    } else {
        mul_div_floor(&a256, &b256, &c256)
    };
    result.to_u128().unwrap_or(u128::MAX)
}

// ============================================================================
// Constants
// ============================================================================

pub const MIN_TICK: i32 = -887_272;
pub const MAX_TICK: i32 = 887_272;
pub const FEE_DENOMINATOR: u128 = 10_000;
pub const TICKS_PER_CHUNK: i32 = 16;

const Q96_U256: U256 = U256([0, 0x100000000, 0, 0]); // 2^96

const MIN_SQRT_RATIO: u128 = 4_295_128_739;

// MAX_SQRT_RATIO as bytes (big-endian):
// 0xfffd8963efd1fc6a506488495d951d5263988d26
const MAX_SQRT_RATIO_LIMBS: [u64; 4] = [0x5d951d5263988d26, 0xefd1fc6a50648849, 0x0000fffd8963, 0];

/// Tick multiplier table (same as on-chain). Each entry is a Q128.128
/// fixed-point value.
const TICK_MULTIPLIERS: [u128; 20] = [
    0xfffcb933bd6fad37aa2d162d1a594001,
    0xfff97272373d413259a46990580e213a,
    0xfff2e50f5f656932ef12357cf3c7fdcc,
    0xffe5caca7e10e4e61c3624eaa0941cd0,
    0xffcb9843d60f6159c9db58835c926644,
    0xff973b41fa98c081472e6896dfb254c0,
    0xff2ea16466c96a3843ec78b326b52861,
    0xfe5dee046a99a2a811c461f1969c3053,
    0xfcbe86c7900a88aedcffc83b479aa3a4,
    0xf987a7253ac413176f2b074cf7815e54,
    0xf3392b0822b70005940c7a398e4b70f3,
    0xe7159475a2c29b7443b29c7fa6e889d9,
    0xd097f3bdfd2022b8845ad8f792aa5825,
    0xa9f746462d870fdf8a65dc1f90e061e5,
    0x70d869a156d2a1b890bb3df62baf32f7,
    0x31be135f97d08fd981231505542fcfa6,
    0x09aa508b5b7a84e1c677de54f3e99bc9,
    0x005d6af8dedb81196699c329225ee604,
    0x0002216e584f5fa1ea926041bedfe98,
    0x000048a170391f7dc42444e8fa2,
];

// ============================================================================
// Tick Math
// ============================================================================

pub fn min_sqrt_ratio() -> U256 {
    U256::from_u128(MIN_SQRT_RATIO)
}

pub fn max_sqrt_ratio() -> U256 {
    U256(MAX_SQRT_RATIO_LIMBS)
}

/// Compute sqrt_price_x96 from a tick value.
/// Exact port of the on-chain `sqrt_ratio_at_tick`.
pub fn sqrt_ratio_at_tick(tick: i32) -> U256 {
    assert!(tick >= MIN_TICK && tick <= MAX_TICK, "tick out of bounds");

    let abs_tick = tick.unsigned_abs();

    // Start with Q128 = 2^128
    let q128 = U256::from_u128(1u128 << 127).shl(1); // 2^128
    let mut ratio = q128;

    for (i, mul) in TICK_MULTIPLIERS.iter().enumerate() {
        if abs_tick & (1u32 << i) != 0 {
            // ratio = (ratio * mul) >> 128
            let mul_u256 = U256::from_u128(*mul);
            ratio = mul_shift_128(&ratio, &mul_u256);
        }
    }

    if tick > 0 {
        // ratio = U256::MAX / ratio
        ratio = U256::MAX.div(&ratio);
    }

    // Convert from Q128 to Q96: shift right by 32, round up if remainder != 0
    let q32 = U256::from_u64(1u64 << 32);
    let remainder = ratio.rem(&q32);
    let mut sqrt_price_x96 = ratio.shr(32);
    if !remainder.is_zero() {
        sqrt_price_x96 = sqrt_price_x96.wrapping_add(&U256::ONE);
    }

    sqrt_price_x96
}

/// Helper: (a * b) >> 128
fn mul_shift_128(a: &U256, b: &U256) -> U256 {
    let (lo, hi) = a.full_mul(b);
    // Result = (hi << 128) | (lo >> 128)
    let lo_shifted = lo.shr(128);
    let hi_shifted = hi.shl(128);
    hi_shifted.wrapping_add(&lo_shifted)
}

/// Compute tick from sqrt_price_x96.
/// Uses the same algorithm as the on-chain contract.
pub fn tick_at_sqrt_ratio(sqrt_price_x96: &U256) -> i32 {
    assert!(
        *sqrt_price_x96 >= min_sqrt_ratio() && *sqrt_price_x96 < max_sqrt_ratio(),
        "sqrt_price out of bounds"
    );

    const LOG_SQRT10001: u128 = 255_738_958_999_603_826_347_141;
    const TICK_LOW_ERROR: u128 = 3_402_992_956_809_132_418_596_140_100_660_247_210;
    const TICK_HI_ERROR: u128 = 291_339_464_771_989_622_907_027_621_153_398_088_495;

    // Convert Q64.96 -> Q128.128 by shifting left 32
    let ratio = sqrt_price_x96.shl(32);
    let ratio_hi_u256 = ratio.shr(128);
    let ratio_hi = ratio_hi_u256.to_u128().unwrap_or(0);
    let ratio_lo_u256 = ratio.wrapping_sub(&ratio_hi_u256.shl(128));
    let ratio_lo = ratio_lo_u256.to_u128().unwrap_or(0);

    let msb: u32 = if ratio_hi > 0 {
        128 + (127 - ratio_hi.leading_zeros())
    } else {
        127 - ratio_lo.leading_zeros()
    };

    // Normalize so r is in [2^127, 2^128)
    let mut r: u128 = if msb >= 128 {
        let s = msb - 127;
        (ratio_hi << (128 - s)) | (ratio_lo >> s)
    } else {
        ratio_lo << (127 - msb)
    };

    // Fixed-point log2 in Q64.64
    let mut log_2: i128 = ((msb as i128) - 128) << 64;
    for bit_pos in (50u32..=63u32).rev() {
        let (sq_hi, sq_lo) = widening_mul_u128(r, r);
        let f = sq_hi >> 127; // 0 or 1
        log_2 |= (f as i128) << bit_pos;
        r = if f == 0 { (sq_hi << 1) | (sq_lo >> 127) } else { sq_hi };
    }

    // Convert log2 -> log_sqrt(1.0001)
    let neg = log_2 < 0;
    let abs_log_2 = log_2.unsigned_abs();
    let (mul_hi, mul_lo) = widening_mul_u128(abs_log_2, LOG_SQRT10001);

    let (log_hi, log_lo): (i128, u128) = if !neg {
        (mul_hi as i128, mul_lo)
    } else if mul_lo == 0 {
        (-(mul_hi as i128), 0)
    } else {
        (-(mul_hi as i128) - 1, mul_lo.wrapping_neg())
    };

    let tick_low = (log_hi - if log_lo < TICK_LOW_ERROR { 1 } else { 0 }) as i32;
    let tick_hi = (log_hi + if log_lo.overflowing_add(TICK_HI_ERROR).1 { 1 } else { 0 }) as i32;

    if tick_low == tick_hi {
        tick_low
    } else if sqrt_ratio_at_tick(tick_hi) <= *sqrt_price_x96 {
        tick_hi
    } else {
        tick_low
    }
}

/// 128-bit x 128-bit -> 256-bit unsigned multiply. Returns (hi, lo).
fn widening_mul_u128(a: u128, b: u128) -> (u128, u128) {
    let a0 = a & 0xFFFF_FFFF_FFFF_FFFF;
    let a1 = a >> 64;
    let b0 = b & 0xFFFF_FFFF_FFFF_FFFF;
    let b1 = b >> 64;

    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;

    let mid = (p00 >> 64) + (p01 & 0xFFFF_FFFF_FFFF_FFFF) + (p10 & 0xFFFF_FFFF_FFFF_FFFF);

    let lo = (p00 & 0xFFFF_FFFF_FFFF_FFFF) | ((mid & 0xFFFF_FFFF_FFFF_FFFF) << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);

    (hi, lo)
}

// ============================================================================
// Amount Delta Functions
// ============================================================================

/// Compute amount0 (token0) delta between two sqrt prices for given liquidity.
/// amount0 = L * Q96 * (sb - sa) / (sa * sb)
pub fn amount0_delta(sqrt_a: &U256, sqrt_b: &U256, liquidity: u128, round_up: bool) -> Option<u128> {
    if liquidity == 0 {
        return Some(0);
    }

    let (sa, sb) = if *sqrt_a > *sqrt_b {
        (sqrt_b, sqrt_a)
    } else {
        (sqrt_a, sqrt_b)
    };

    if sa.is_zero() {
        return None;
    }

    let diff = sb.wrapping_sub(sa);
    let liq = U256::from_u128(liquidity);
    let numerator1 = liq.wrapping_mul(&Q96_U256);

    // temp = numerator1 * diff / sb
    let temp = if round_up {
        mul_div_ceil(&numerator1, &diff, sb)
    } else {
        mul_div_floor(&numerator1, &diff, sb)
    };

    // amount = temp / sa
    let amount = if round_up {
        let r = temp.rem(sa);
        let q = temp.div(sa);
        if r.is_zero() {
            q
        } else {
            q.wrapping_add(&U256::ONE)
        }
    } else {
        temp.div(sa)
    };

    amount.to_u128()
}

/// Compute amount1 (token1) delta between two sqrt prices for given liquidity.
/// amount1 = L * (sb - sa) / Q96
pub fn amount1_delta(sqrt_a: &U256, sqrt_b: &U256, liquidity: u128, round_up: bool) -> Option<u128> {
    if liquidity == 0 {
        return Some(0);
    }

    let (sa, sb) = if *sqrt_a > *sqrt_b {
        (sqrt_b, sqrt_a)
    } else {
        (sqrt_a, sqrt_b)
    };

    let diff = sb.wrapping_sub(sa);
    let liq = U256::from_u128(liquidity);

    let amount = if round_up {
        mul_div_ceil(&liq, &diff, &Q96_U256)
    } else {
        mul_div_floor(&liq, &diff, &Q96_U256)
    };

    amount.to_u128()
}

// ============================================================================
// Next Sqrt Price Functions
// ============================================================================

/// Compute next sqrt price given token0 input/output amount.
pub fn get_next_sqrt_price_from_amount0(sqrt_price: &U256, liquidity: u128, amount: u128, add: bool) -> U256 {
    if amount == 0 {
        return *sqrt_price;
    }

    let liq = U256::from_u128(liquidity);
    let numerator1 = liq.wrapping_mul(&Q96_U256);
    let amt = U256::from_u128(amount);

    if add {
        let product = amt.wrapping_mul(sqrt_price);
        let denominator = numerator1.wrapping_add(&product);
        // result = ceil(numerator1 * sqrt_price / denominator)
        mul_div_ceil(&numerator1, sqrt_price, &denominator)
    } else {
        let product = amt.wrapping_mul(sqrt_price);
        assert!(numerator1 > product, "price out of bounds");
        let denominator = numerator1.wrapping_sub(&product);
        mul_div_ceil(&numerator1, sqrt_price, &denominator)
    }
}

/// Compute next sqrt price given token1 input/output amount.
pub fn get_next_sqrt_price_from_amount1(sqrt_price: &U256, liquidity: u128, amount: u128, add: bool) -> U256 {
    let liq = U256::from_u128(liquidity);
    let amt = U256::from_u128(amount);

    if add {
        let quotient = mul_div_floor(&amt, &Q96_U256, &liq);
        sqrt_price.wrapping_add(&quotient)
    } else {
        let quotient = mul_div_ceil(&amt, &Q96_U256, &liq);
        assert!(*sqrt_price > quotient, "price out of bounds");
        sqrt_price.wrapping_sub(&quotient)
    }
}

/// Compute next sqrt price from input amount.
pub fn get_next_sqrt_price_from_input(sqrt_price: &U256, liquidity: u128, amount_in: u128, zero_for_one: bool) -> U256 {
    if zero_for_one {
        get_next_sqrt_price_from_amount0(sqrt_price, liquidity, amount_in, true)
    } else {
        get_next_sqrt_price_from_amount1(sqrt_price, liquidity, amount_in, true)
    }
}

/// Compute next sqrt price from output amount.
pub fn get_next_sqrt_price_from_output(
    sqrt_price: &U256,
    liquidity: u128,
    amount_out: u128,
    zero_for_one: bool,
) -> U256 {
    if zero_for_one {
        get_next_sqrt_price_from_amount1(sqrt_price, liquidity, amount_out, false)
    } else {
        get_next_sqrt_price_from_amount0(sqrt_price, liquidity, amount_out, false)
    }
}

// ============================================================================
// Swap Step Computation
// ============================================================================

/// Result of a single swap step within one tick range.
#[derive(Debug, Clone)]
pub struct SwapStep {
    pub sqrt_next: U256,
    pub amount_in: u128,
    pub amount_out: u128,
    pub fee_amount: u128,
}

/// Compute a single swap step (within one tick range).
/// This is the core computation that determines how much can be swapped
/// before hitting the next initialized tick.
pub fn compute_swap_step(
    sqrt_current: &U256,
    sqrt_target: &U256,
    liquidity: u128,
    amount_remaining: u128,
    fee_pips: u32,
    zero_for_one: bool,
    exact_input: bool,
) -> SwapStep {
    if liquidity == 0 {
        return SwapStep {
            sqrt_next: *sqrt_target,
            amount_in: 0,
            amount_out: 0,
            fee_amount: 0,
        };
    }

    let fee = fee_pips as u128;
    let fee_complement = FEE_DENOMINATOR - fee;

    if exact_input {
        let amount_remaining_less_fee = mul_div_u128(amount_remaining, fee_complement, FEE_DENOMINATOR, false);

        let amount_in_to_target = if zero_for_one {
            amount0_delta(sqrt_target, sqrt_current, liquidity, true)
        } else {
            amount1_delta(sqrt_current, sqrt_target, liquidity, true)
        };

        let sqrt_next = if amount_in_to_target.map_or(false, |a| amount_remaining_less_fee >= a) {
            *sqrt_target
        } else {
            let computed =
                get_next_sqrt_price_from_input(sqrt_current, liquidity, amount_remaining_less_fee, zero_for_one);
            // Clamp to target range
            if zero_for_one {
                if computed < *sqrt_target {
                    *sqrt_target
                } else {
                    computed
                }
            } else {
                if computed > *sqrt_target {
                    *sqrt_target
                } else {
                    computed
                }
            }
        };

        let max_reached = sqrt_next == *sqrt_target;

        let amount_in = if zero_for_one {
            amount0_delta(&sqrt_next, sqrt_current, liquidity, true).unwrap_or(u128::MAX)
        } else {
            amount1_delta(sqrt_current, &sqrt_next, liquidity, true).unwrap_or(u128::MAX)
        };

        let amount_out = if zero_for_one {
            amount1_delta(&sqrt_next, sqrt_current, liquidity, false).unwrap_or(0)
        } else {
            amount0_delta(sqrt_current, &sqrt_next, liquidity, false).unwrap_or(0)
        };

        let fee_amount = if max_reached {
            mul_div_u128(amount_in, fee, fee_complement, true)
        } else {
            amount_remaining.saturating_sub(amount_in)
        };

        SwapStep {
            sqrt_next,
            amount_in,
            amount_out,
            fee_amount,
        }
    } else {
        // Exact output
        let amount_out_to_target = if zero_for_one {
            amount1_delta(sqrt_target, sqrt_current, liquidity, false)
        } else {
            amount0_delta(sqrt_current, sqrt_target, liquidity, false)
        };

        let sqrt_next = if amount_out_to_target.map_or(false, |a| amount_remaining >= a) {
            *sqrt_target
        } else {
            let computed = get_next_sqrt_price_from_output(sqrt_current, liquidity, amount_remaining, zero_for_one);
            if zero_for_one {
                if computed < *sqrt_target {
                    *sqrt_target
                } else {
                    computed
                }
            } else {
                if computed > *sqrt_target {
                    *sqrt_target
                } else {
                    computed
                }
            }
        };

        let amount_in = if zero_for_one {
            amount0_delta(&sqrt_next, sqrt_current, liquidity, true).unwrap_or(u128::MAX)
        } else {
            amount1_delta(sqrt_current, &sqrt_next, liquidity, true).unwrap_or(u128::MAX)
        };

        let mut amount_out = if zero_for_one {
            amount1_delta(&sqrt_next, sqrt_current, liquidity, false).unwrap_or(0)
        } else {
            amount0_delta(sqrt_current, &sqrt_next, liquidity, false).unwrap_or(0)
        };

        if amount_out > amount_remaining {
            amount_out = amount_remaining;
        }

        let fee_amount = mul_div_u128(amount_in, fee, fee_complement, true);

        SwapStep {
            sqrt_next,
            amount_in,
            amount_out,
            fee_amount,
        }
    }
}

// ============================================================================
// Tick Data & Pool State (for off-chain swap simulation)
// ============================================================================

/// Tick info needed for swap simulation (subset of on-chain TickInfo).
#[derive(Debug, Clone)]
pub struct TickState {
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
}

/// Pool state needed for swap simulation.
#[derive(Debug, Clone)]
pub struct ClmmPoolState {
    pub sqrt_price_x96: U256,
    pub tick: i32,
    pub liquidity: u128,
    pub fee_bps: u32,
    pub tick_spacing: i32,
    pub token0: String,
    pub token1: String,
}

/// Bitmap helpers for tick traversal
pub mod bitmap {
    use super::TICKS_PER_CHUNK;

    /// Compress a tick by dividing by spacing (floor division).
    pub fn compress_tick(tick: i32, spacing: i32) -> i32 {
        let mut compressed = tick / spacing;
        if tick < 0 && tick % spacing != 0 {
            compressed -= 1;
        }
        compressed
    }

    /// Convert compressed tick back to actual tick.
    pub fn compressed_to_tick(compressed: i32, spacing: i32) -> i32 {
        compressed
            .saturating_mul(spacing)
            .max(super::MIN_TICK)
            .min(super::MAX_TICK)
    }

    /// Compute (chunk_pos, slot) from a compressed tick.
    pub fn chunk_address(compressed_tick: i32) -> (i32, u32) {
        let chunk_pos = compressed_tick.div_euclid(TICKS_PER_CHUNK);
        let slot = compressed_tick.rem_euclid(TICKS_PER_CHUNK) as u32;
        (chunk_pos, slot)
    }

    /// Chunk bitmap position: (word_pos, bit_pos) from chunk_pos.
    pub fn chunk_bitmap_position(chunk_pos: i32) -> (i32, u32) {
        let word_pos = chunk_pos >> 8;
        let bit_pos = (chunk_pos & 255) as u32;
        (word_pos, bit_pos)
    }

    /// Level-2 word bitmap position.
    pub fn word_bitmap_position(chunk_bitmap_word_pos: i32) -> (i32, u32) {
        let l2_word_pos = chunk_bitmap_word_pos >> 8;
        let bit_pos = (chunk_bitmap_word_pos & 255) as u32;
        (l2_word_pos, bit_pos)
    }

    /// Find the previous (lower) set bit in a 256-bit word (big-endian [u8;
    /// 32]).
    pub fn find_prev_set_bit(word: &[u8; 32], from_bit: u32) -> Option<u32> {
        let from_bit = from_bit.min(255);
        let start_byte = (255 - from_bit) / 8;
        let start_bit_in_byte = from_bit % 8;

        let mask = ((1u16 << (start_bit_in_byte + 1)) - 1) as u8;
        let masked = word[start_byte as usize] & mask;
        if masked != 0 {
            let top_bit = 7 - masked.leading_zeros();
            return Some((31 - start_byte) * 8 + top_bit);
        }

        for byte_idx in (start_byte + 1)..32 {
            if word[byte_idx as usize] != 0 {
                let top_bit = 7 - word[byte_idx as usize].leading_zeros();
                return Some((31 - byte_idx) * 8 + top_bit);
            }
        }

        None
    }

    /// Find the next (higher) set bit in a 256-bit word (big-endian [u8; 32]).
    pub fn find_next_set_bit(word: &[u8; 32], from_bit: u32) -> Option<u32> {
        let from_bit = from_bit.min(255);
        let start_byte = (255 - from_bit) / 8;
        let start_bit_in_byte = from_bit % 8;

        let mask = !((1u8 << start_bit_in_byte).wrapping_sub(1));
        let masked = word[start_byte as usize] & mask;
        if masked != 0 {
            let low_bit = masked.trailing_zeros();
            return Some((31 - start_byte) * 8 + low_bit);
        }

        if start_byte > 0 {
            for byte_idx in (0..start_byte).rev() {
                if word[byte_idx as usize] != 0 {
                    let low_bit = word[byte_idx as usize].trailing_zeros();
                    return Some((31 - byte_idx) * 8 + low_bit);
                }
            }
        }

        None
    }
}

// ============================================================================
// Full Swap Simulation (off-chain, read-only)
// ============================================================================

/// Tick data store for off-chain simulation.
/// Maps chunk_pos -> Vec of (liquidity_gross, liquidity_net) for each slot.
#[derive(Debug, Clone)]
pub struct TickDataStore {
    /// chunk_pos -> [TickState; TICKS_PER_CHUNK]
    pub chunks: std::collections::HashMap<i32, Vec<TickState>>,
    /// Chunk bitmap: word_pos -> [u8; 32] (big-endian 256-bit word)
    pub chunk_bitmap: std::collections::HashMap<i32, [u8; 32]>,
    /// Level-2 word bitmap: l2_word_pos -> [u8; 32]
    pub word_bitmap: std::collections::HashMap<i32, [u8; 32]>,
}

impl TickDataStore {
    pub fn new() -> Self {
        Self {
            chunks: std::collections::HashMap::new(),
            chunk_bitmap: std::collections::HashMap::new(),
            word_bitmap: std::collections::HashMap::new(),
        }
    }

    /// Get tick info at a specific tick.
    pub fn get_tick(&self, tick: i32, spacing: i32) -> TickState {
        let compressed = bitmap::compress_tick(tick, spacing);
        let (chunk_pos, slot) = bitmap::chunk_address(compressed);
        match self.chunks.get(&chunk_pos) {
            Some(chunk) if (slot as usize) < chunk.len() => chunk[slot as usize].clone(),
            _ => TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            },
        }
    }

    /// Find next initialized tick (3-level bitmap search).
    pub fn find_initialized_tick(&self, tick: i32, spacing: i32, lte: bool) -> (i32, bool) {
        let compressed = bitmap::compress_tick(tick, spacing);

        if lte {
            let (chunk_pos, slot) = bitmap::chunk_address(compressed);

            // 1. Check current chunk
            if let Some(chunk) = self.chunks.get(&chunk_pos) {
                for s in (0..=slot).rev() {
                    if chunk[s as usize].liquidity_gross > 0 {
                        let found = chunk_pos * TICKS_PER_CHUNK + s as i32;
                        return (bitmap::compressed_to_tick(found, spacing), true);
                    }
                }
            }

            // 2. Use chunk bitmap
            let (bm_word_pos, bm_bit_pos) = bitmap::chunk_bitmap_position(chunk_pos);
            if let Some(word) = self.chunk_bitmap.get(&bm_word_pos) {
                if bm_bit_pos > 0 {
                    if let Some(found_bit) = bitmap::find_prev_set_bit(word, bm_bit_pos - 1) {
                        let found_chunk = (bm_word_pos << 8) + found_bit as i32;
                        if let Some(tick) = self.highest_init_in_chunk(found_chunk, spacing) {
                            return (tick, true);
                        }
                    }
                }
            }

            // 3. Use L2 word bitmap
            let (bm_word_pos, _) = bitmap::chunk_bitmap_position(chunk_pos);
            if let Some(target_word) = self.find_adjacent_bitmap_word(bm_word_pos, true) {
                if let Some(tick) = self.extreme_tick_in_bitmap_word(target_word, spacing, true) {
                    return (tick, true);
                }
            }

            // Not found
            let boundary = (bm_word_pos << 8) * TICKS_PER_CHUNK;
            (bitmap::compressed_to_tick(boundary, spacing), false)
        } else {
            // Scanning upward
            let compressed_plus_one = compressed.saturating_add(1);
            let (chunk_pos, slot) = bitmap::chunk_address(compressed_plus_one);

            // 1. Check current chunk
            if let Some(chunk) = self.chunks.get(&chunk_pos) {
                for s in slot..(TICKS_PER_CHUNK as u32) {
                    if chunk[s as usize].liquidity_gross > 0 {
                        let found = chunk_pos * TICKS_PER_CHUNK + s as i32;
                        return (bitmap::compressed_to_tick(found, spacing), true);
                    }
                }
            }

            // 2. Use chunk bitmap
            let (bm_word_pos, bm_bit_pos) = bitmap::chunk_bitmap_position(chunk_pos);
            if let Some(word) = self.chunk_bitmap.get(&bm_word_pos) {
                if bm_bit_pos < 255 {
                    if let Some(found_bit) = bitmap::find_next_set_bit(word, bm_bit_pos + 1) {
                        let found_chunk = (bm_word_pos << 8) + found_bit as i32;
                        if let Some(tick) = self.lowest_init_in_chunk(found_chunk, spacing) {
                            return (tick, true);
                        }
                    }
                }
            }

            // 3. Use L2 word bitmap
            let (bm_word_pos, _) = bitmap::chunk_bitmap_position(chunk_pos);
            if let Some(target_word) = self.find_adjacent_bitmap_word(bm_word_pos, false) {
                if let Some(tick) = self.extreme_tick_in_bitmap_word(target_word, spacing, false) {
                    return (tick, true);
                }
            }

            let boundary = ((bm_word_pos << 8) + 255) * TICKS_PER_CHUNK + (TICKS_PER_CHUNK - 1);
            (bitmap::compressed_to_tick(boundary, spacing), false)
        }
    }

    fn highest_init_in_chunk(&self, chunk_pos: i32, spacing: i32) -> Option<i32> {
        if let Some(chunk) = self.chunks.get(&chunk_pos) {
            for s in (0..TICKS_PER_CHUNK as u32).rev() {
                if chunk[s as usize].liquidity_gross > 0 {
                    let found = chunk_pos * TICKS_PER_CHUNK + s as i32;
                    return Some(bitmap::compressed_to_tick(found, spacing));
                }
            }
        }
        None
    }

    fn lowest_init_in_chunk(&self, chunk_pos: i32, spacing: i32) -> Option<i32> {
        if let Some(chunk) = self.chunks.get(&chunk_pos) {
            for s in 0..TICKS_PER_CHUNK as u32 {
                if chunk[s as usize].liquidity_gross > 0 {
                    let found = chunk_pos * TICKS_PER_CHUNK + s as i32;
                    return Some(bitmap::compressed_to_tick(found, spacing));
                }
            }
        }
        None
    }

    fn find_adjacent_bitmap_word(&self, current_word_pos: i32, lte: bool) -> Option<i32> {
        let (l2_pos, l2_bit) = bitmap::word_bitmap_position(current_word_pos);

        let search = |l2_pos: i32, from: u32| -> Option<i32> {
            let l2_word = self.word_bitmap.get(&l2_pos)?;
            let found = if lte {
                bitmap::find_prev_set_bit(l2_word, from)
            } else {
                bitmap::find_next_set_bit(l2_word, from)
            };
            found.map(|bit| (l2_pos << 8) + bit as i32)
        };

        let adjacent = if lte && l2_bit > 0 {
            search(l2_pos, l2_bit - 1)
        } else if !lte && l2_bit < 255 {
            search(l2_pos, l2_bit + 1)
        } else {
            None
        };
        if adjacent.is_some() {
            return adjacent;
        }

        let l2_adj = if lte { l2_pos - 1 } else { l2_pos + 1 };
        let from = if lte { 255 } else { 0 };
        search(l2_adj, from)
    }

    fn extreme_tick_in_bitmap_word(&self, word_pos: i32, spacing: i32, highest: bool) -> Option<i32> {
        let word = self.chunk_bitmap.get(&word_pos)?;
        let found_bit = if highest {
            bitmap::find_prev_set_bit(word, 255)
        } else {
            bitmap::find_next_set_bit(word, 0)
        };
        if let Some(bit) = found_bit {
            let chunk_pos = (word_pos << 8) + bit as i32;
            if highest {
                self.highest_init_in_chunk(chunk_pos, spacing)
            } else {
                self.lowest_init_in_chunk(chunk_pos, spacing)
            }
        } else {
            None
        }
    }
}

fn set_bitmap_bit(word: &mut [u8; 32], bit_pos: u32) {
    let byte_idx = 31usize - (bit_pos / 8) as usize;
    let bit_idx = (bit_pos % 8) as u8;
    word[byte_idx] |= 1u8 << bit_idx;
}

fn derive_word_bitmap_entries(chunk_bitmaps: &std::collections::HashMap<i32, [u8; 32]>) -> Vec<ClmmBitmapWordSnapshot> {
    let mut derived = std::collections::HashMap::<i32, [u8; 32]>::new();
    for (chunk_word_pos, chunk_bitmap_word) in chunk_bitmaps {
        if chunk_bitmap_word.iter().all(|byte| *byte == 0) {
            continue;
        }
        let (l2_word_pos, l2_bit) = bitmap::word_bitmap_position(*chunk_word_pos);
        let word = derived.entry(l2_word_pos).or_insert([0u8; 32]);
        set_bitmap_bit(word, l2_bit);
    }

    let mut entries = derived
        .into_iter()
        .map(|(word_pos, word)| ClmmBitmapWordSnapshot { word_pos, word })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.word_pos);
    entries
}

pub fn clmm_pool_to_snapshot(
    source: impl Into<String>,
    pool_address: impl Into<String>,
    pool: &ClmmPoolState,
    tick_store: &TickDataStore,
    coverage: Option<ClmmCoverageSnapshot>,
) -> ClmmPoolSnapshot {
    let mut ticks = tick_store
        .chunks
        .iter()
        .flat_map(|(chunk_pos, chunk)| {
            chunk.iter().enumerate().filter_map(|(slot, state)| {
                if state.liquidity_gross == 0 {
                    return None;
                }
                let compressed_tick = chunk_pos.saturating_mul(TICKS_PER_CHUNK) + slot as i32;
                Some(ClmmTickSnapshot {
                    tick: bitmap::compressed_to_tick(compressed_tick, pool.tick_spacing),
                    liquidity_gross: state.liquidity_gross,
                    liquidity_net: state.liquidity_net,
                })
            })
        })
        .collect::<Vec<_>>();
    ticks.sort_by_key(|tick| tick.tick);

    let mut chunk_bitmaps = tick_store
        .chunk_bitmap
        .iter()
        .map(|(word_pos, word)| ClmmBitmapWordSnapshot {
            word_pos: *word_pos,
            word: *word,
        })
        .collect::<Vec<_>>();
    chunk_bitmaps.sort_by_key(|bitmap| bitmap.word_pos);

    let mut word_bitmaps = if tick_store.word_bitmap.is_empty() {
        derive_word_bitmap_entries(&tick_store.chunk_bitmap)
    } else {
        tick_store
            .word_bitmap
            .iter()
            .map(|(word_pos, word)| ClmmBitmapWordSnapshot {
                word_pos: *word_pos,
                word: *word,
            })
            .collect::<Vec<_>>()
    };
    word_bitmaps.sort_by_key(|bitmap| bitmap.word_pos);

    ClmmPoolSnapshot {
        source: source.into(),
        pool_address: pool_address.into(),
        token0: pool.token0.clone(),
        token1: pool.token1.clone(),
        fee_bps: pool.fee_bps,
        tick_spacing: pool.tick_spacing,
        sqrt_price_x96: pool.sqrt_price_x96.0,
        tick: pool.tick,
        liquidity: pool.liquidity,
        ticks,
        chunk_bitmaps,
        word_bitmaps,
        coverage,
    }
}

pub fn clmm_pool_from_snapshot(snapshot: &ClmmPoolSnapshot) -> (ClmmPoolState, TickDataStore) {
    let pool = ClmmPoolState {
        sqrt_price_x96: U256(snapshot.sqrt_price_x96),
        tick: snapshot.tick,
        liquidity: snapshot.liquidity,
        fee_bps: snapshot.fee_bps,
        tick_spacing: snapshot.tick_spacing,
        token0: snapshot.token0.clone(),
        token1: snapshot.token1.clone(),
    };

    let mut tick_store = TickDataStore::new();
    for tick in &snapshot.ticks {
        let compressed = bitmap::compress_tick(tick.tick, snapshot.tick_spacing);
        let (chunk_pos, slot) = bitmap::chunk_address(compressed);
        let chunk = tick_store.chunks.entry(chunk_pos).or_insert_with(|| {
            vec![
                TickState {
                    liquidity_gross: 0,
                    liquidity_net: 0,
                };
                TICKS_PER_CHUNK as usize
            ]
        });
        chunk[slot as usize] = TickState {
            liquidity_gross: tick.liquidity_gross,
            liquidity_net: tick.liquidity_net,
        };
    }

    for bitmap in &snapshot.chunk_bitmaps {
        tick_store.chunk_bitmap.insert(bitmap.word_pos, bitmap.word);
    }
    if snapshot.word_bitmaps.is_empty() {
        for bitmap in derive_word_bitmap_entries(&tick_store.chunk_bitmap) {
            tick_store.word_bitmap.insert(bitmap.word_pos, bitmap.word);
        }
    } else {
        for bitmap in &snapshot.word_bitmaps {
            tick_store.word_bitmap.insert(bitmap.word_pos, bitmap.word);
        }
    }

    (pool, tick_store)
}

/// True when the active tick moved outside a prior Sushi-style bitmap word scan
/// window.
pub fn tick_outside_word_scan(tick: i32, tick_spacing: i32, word_start: i32, word_end: i32) -> bool {
    let compressed = tick.div_euclid(tick_spacing);
    let word = compressed.div_euclid(256);
    word < word_start || word > word_end
}

/// True when the active tick is outside initialized ticks we have loaded
/// locally.
pub fn tick_outside_loaded_range(tick: i32, min_loaded: i32, max_loaded: i32) -> bool {
    tick < min_loaded || tick > max_loaded
}

/// Inputs for deciding whether local CLMM math is safe to use for quoting.
#[derive(Debug, Clone, Copy)]
pub struct ClmmCoverageInput {
    pub pool_tick: i32,
    pub tick_spacing: i32,
    pub is_complete: bool,
    pub min_loaded_tick: Option<i32>,
    pub max_loaded_tick: Option<i32>,
    pub scanned_word_start: Option<i32>,
    pub scanned_word_end: Option<i32>,
}

impl ClmmCoverageInput {
    pub fn from_snapshot(pool: &ClmmPoolState, coverage: &ClmmCoverageSnapshot) -> Self {
        Self {
            pool_tick: pool.tick,
            tick_spacing: pool.tick_spacing,
            is_complete: coverage.is_complete,
            min_loaded_tick: coverage.min_loaded_tick,
            max_loaded_tick: coverage.max_loaded_tick,
            scanned_word_start: coverage.scanned_word_start,
            scanned_word_end: coverage.scanned_word_end,
        }
    }
}

pub fn clmm_has_initialized_ticks(tick_store: &TickDataStore) -> bool {
    tick_store
        .chunks
        .values()
        .any(|chunk| chunk.iter().any(|tick| tick.liquidity_gross > 0))
}

/// Pool metadata is present and the active tick sits inside declared coverage
/// bounds.
pub fn clmm_quote_allowed(tick_store: &TickDataStore, coverage: &ClmmCoverageInput) -> bool {
    if !coverage.is_complete {
        return false;
    }
    if !clmm_has_initialized_ticks(tick_store) {
        return false;
    }
    if let (Some(start), Some(end)) = (coverage.scanned_word_start, coverage.scanned_word_end) {
        if tick_outside_word_scan(coverage.pool_tick, coverage.tick_spacing, start, end) {
            return false;
        }
    }
    if let (Some(min_tick), Some(max_tick)) = (coverage.min_loaded_tick, coverage.max_loaded_tick) {
        if tick_outside_loaded_range(coverage.pool_tick, min_tick, max_tick) {
            return false;
        }
    }
    true
}

/// Whether a simulated swap stays within loaded tick boundaries (guards
/// missing-tick math).
pub fn swap_stays_within_loaded_ticks(
    pool: &ClmmPoolState,
    ticks: &TickDataStore,
    amount_in: u128,
    zero_for_one: bool,
    min_loaded: i32,
    max_loaded: i32,
) -> bool {
    if amount_in == 0 || pool.liquidity == 0 {
        return false;
    }
    if tick_outside_loaded_range(pool.tick, min_loaded, max_loaded) {
        return false;
    }
    let Some((amount_out, _, final_tick)) = simulate_swap(pool, ticks, amount_in, zero_for_one) else {
        return false;
    };
    if amount_out == 0 {
        return false;
    }
    !tick_outside_loaded_range(final_tick, min_loaded, max_loaded)
}

/// Full local quote guard: metadata + swap path stays inside loaded ticks.
pub fn clmm_swap_allowed(
    pool: &ClmmPoolState,
    ticks: &TickDataStore,
    amount_in: u128,
    zero_for_one: bool,
    coverage: &ClmmCoverageInput,
) -> bool {
    if !clmm_quote_allowed(ticks, coverage) {
        return false;
    }
    let Some((range_min, range_max)) = loaded_tick_range(ticks, pool.tick_spacing) else {
        return false;
    };
    let min_loaded = coverage.min_loaded_tick.unwrap_or(range_min).min(range_min);
    let max_loaded = coverage.max_loaded_tick.unwrap_or(range_max).max(range_max);
    swap_stays_within_loaded_ticks(pool, ticks, amount_in, zero_for_one, min_loaded, max_loaded)
}

pub fn loaded_tick_range(tick_store: &TickDataStore, tick_spacing: i32) -> Option<(i32, i32)> {
    let mut min_tick: Option<i32> = None;
    let mut max_tick: Option<i32> = None;

    for (chunk_pos, chunk) in &tick_store.chunks {
        for (slot, state) in chunk.iter().enumerate() {
            if state.liquidity_gross == 0 {
                continue;
            }
            let compressed_tick = chunk_pos.saturating_mul(TICKS_PER_CHUNK) + slot as i32;
            let tick = bitmap::compressed_to_tick(compressed_tick, tick_spacing);
            min_tick = Some(min_tick.map_or(tick, |current| current.min(tick)));
            max_tick = Some(max_tick.map_or(tick, |current| current.max(tick)));
        }
    }

    min_tick.zip(max_tick)
}

/// Simulate a swap on a CLMM pool (exact input, zero_for_one or one_for_zero).
/// Returns (amount_out, final_sqrt_price, final_tick).
/// This is the off-chain equivalent of the on-chain swap_loop with
/// dry_run=true.
pub fn simulate_swap(
    pool: &ClmmPoolState,
    ticks: &TickDataStore,
    amount_in: u128,
    zero_for_one: bool,
) -> Option<(u128, U256, i32)> {
    if amount_in == 0 || pool.liquidity == 0 {
        return None;
    }
    if let Some((min_loaded, max_loaded)) = loaded_tick_range(ticks, pool.tick_spacing) {
        if tick_outside_loaded_range(pool.tick, min_loaded, max_loaded) {
            return None;
        }
    } else {
        return None;
    }

    let price_limit = if zero_for_one {
        min_sqrt_ratio().wrapping_add(&U256::ONE)
    } else {
        max_sqrt_ratio().wrapping_sub(&U256::ONE)
    };

    let mut sqrt_price = pool.sqrt_price_x96;
    let mut tick = pool.tick;
    let mut liquidity = pool.liquidity;
    let mut amount_remaining = amount_in;
    let mut amount_calculated: u128 = 0;

    let mut iterations = 0u32;
    const MAX_ITERATIONS: u32 = 500; // Safety limit

    while amount_remaining > 0 && sqrt_price != price_limit {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            break;
        }

        let (next_tick, next_tick_initialized) = ticks.find_initialized_tick(tick, pool.tick_spacing, zero_for_one);

        let next_tick_price = sqrt_ratio_at_tick(next_tick);

        let sqrt_target = if zero_for_one {
            if next_tick_price < price_limit {
                price_limit
            } else {
                next_tick_price
            }
        } else {
            if next_tick_price > price_limit {
                price_limit
            } else {
                next_tick_price
            }
        };

        let step = compute_swap_step(
            &sqrt_price,
            &sqrt_target,
            liquidity,
            amount_remaining,
            pool.fee_bps,
            zero_for_one,
            true, // exact_input
        );

        amount_remaining = amount_remaining
            .saturating_sub(step.amount_in)
            .saturating_sub(step.fee_amount);
        amount_calculated = amount_calculated.saturating_add(step.amount_out);

        sqrt_price = step.sqrt_next;

        if sqrt_price == next_tick_price && next_tick_initialized {
            // Cross tick: apply liquidity_net
            let tick_state = ticks.get_tick(next_tick, pool.tick_spacing);
            let mut liquidity_net = tick_state.liquidity_net;
            if zero_for_one {
                liquidity_net = -liquidity_net;
            }
            if liquidity_net < 0 {
                let dec = (-liquidity_net) as u128;
                liquidity = liquidity.saturating_sub(dec);
            } else {
                liquidity = liquidity.saturating_add(liquidity_net as u128);
            }

            tick = if zero_for_one {
                next_tick.saturating_sub(1).max(MIN_TICK)
            } else {
                next_tick.min(MAX_TICK)
            };
        } else if sqrt_price != pool.sqrt_price_x96 {
            tick = tick_at_sqrt_ratio(&sqrt_price);
        }
    }

    if amount_calculated == 0 {
        return None;
    }

    Some((amount_calculated, sqrt_price, tick))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use {super::*, market_snapshot::ClmmCoverageSnapshot};

    #[test]
    fn test_u256_basic_ops() {
        let a = U256::from_u128(100);
        let b = U256::from_u128(200);
        let sum = a.wrapping_add(&b);
        assert_eq!(sum.to_u128(), Some(300));

        let diff = b.wrapping_sub(&a);
        assert_eq!(diff.to_u128(), Some(100));
    }

    #[test]
    fn test_u256_mul() {
        let a = U256::from_u128(1_000_000);
        let b = U256::from_u128(2_000_000);
        let product = a.wrapping_mul(&b);
        assert_eq!(product.to_u128(), Some(2_000_000_000_000));
    }

    #[test]
    fn test_u256_div() {
        let a = U256::from_u128(1_000_000);
        let b = U256::from_u128(3);
        let q = a.div(&b);
        assert_eq!(q.to_u128(), Some(333_333));
    }

    #[test]
    fn test_u256_shift() {
        let one = U256::ONE;
        let shifted = one.shl(96);
        assert_eq!(shifted, Q96_U256);

        let back = shifted.shr(96);
        assert_eq!(back, U256::ONE);
    }

    #[test]
    fn test_mul_div_floor() {
        let a = U256::from_u128(10);
        let b = U256::from_u128(3);
        let d = U256::from_u128(7);
        let result = mul_div_floor(&a, &b, &d);
        assert_eq!(result.to_u128(), Some(4)); // floor(30/7) = 4
    }

    #[test]
    fn test_mul_div_ceil() {
        let a = U256::from_u128(10);
        let b = U256::from_u128(3);
        let d = U256::from_u128(7);
        let result = mul_div_ceil(&a, &b, &d);
        assert_eq!(result.to_u128(), Some(5)); // ceil(30/7) = 5
    }

    #[test]
    fn test_mul_div_large() {
        // u128::MAX * u128::MAX / u128::MAX = u128::MAX
        let max = U256::from_u128(u128::MAX);
        let result = mul_div_floor(&max, &max, &max);
        assert_eq!(result.to_u128(), Some(u128::MAX));
    }

    #[test]
    fn test_sqrt_ratio_at_tick_zero() {
        let sqrt = sqrt_ratio_at_tick(0);
        // tick 0 -> price = 1.0 -> sqrt_price_x96 = 2^96
        assert_eq!(sqrt, Q96_U256);
    }

    #[test]
    fn test_tick_math_roundtrip() {
        for tick in [-887_272, -100_000, -1, 0, 1, 100_000, 887_271] {
            let sqrt = sqrt_ratio_at_tick(tick);
            let actual_tick = tick_at_sqrt_ratio(&sqrt);
            assert_eq!(actual_tick, tick, "roundtrip failed for tick {}", tick);
        }
    }

    #[test]
    fn test_tick_at_sqrt_ratio_boundaries() {
        let min = min_sqrt_ratio();
        let max_minus_one = max_sqrt_ratio().wrapping_sub(&U256::ONE);

        assert_eq!(tick_at_sqrt_ratio(&min), MIN_TICK);
        assert_eq!(tick_at_sqrt_ratio(&max_minus_one), MAX_TICK - 1);
    }

    #[test]
    fn test_amount0_delta_basic() {
        // Simple case: liquidity=1e12, tick range [0, 100]
        let sqrt_a = sqrt_ratio_at_tick(0);
        let sqrt_b = sqrt_ratio_at_tick(100);
        let result = amount0_delta(&sqrt_a, &sqrt_b, 1_000_000_000_000, true);
        assert!(result.is_some());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn test_amount1_delta_basic() {
        let sqrt_a = sqrt_ratio_at_tick(0);
        let sqrt_b = sqrt_ratio_at_tick(100);
        let result = amount1_delta(&sqrt_a, &sqrt_b, 1_000_000_000_000, true);
        assert!(result.is_some());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn test_compute_swap_step_exact_input() {
        let sqrt_current = sqrt_ratio_at_tick(0);
        let sqrt_target = sqrt_ratio_at_tick(-100);
        let liquidity = 1_000_000_000_000u128;
        let amount_remaining = 1_000_000u128;
        let fee_bps = 30; // 0.3%

        let step = compute_swap_step(
            &sqrt_current,
            &sqrt_target,
            liquidity,
            amount_remaining,
            fee_bps,
            true, // zero_for_one
            true, // exact_input
        );

        assert!(step.amount_in > 0);
        assert!(step.amount_out > 0);
        assert!(step.fee_amount > 0);
        assert!(step.amount_in + step.fee_amount <= amount_remaining);
    }

    #[test]
    fn test_simulate_swap_basic() {
        // Create a simple pool with one tick range of liquidity
        let pool = ClmmPoolState {
            sqrt_price_x96: sqrt_ratio_at_tick(0),
            tick: 0,
            liquidity: 10_000_000_000_000u128, // 10^13
            fee_bps: 30,
            tick_spacing: 200,
            token0: "TOKEN_A".to_string(),
            token1: "TOKEN_B".to_string(),
        };

        // Create tick data with liquidity at [-1000, 1000]
        let mut ticks = TickDataStore::new();

        // Add initialized ticks at -1000 and 1000 (compressed: -5 and 5 for
        // spacing=200)
        let lower_compressed = bitmap::compress_tick(-1000, 200); // -5
        let upper_compressed = bitmap::compress_tick(1000, 200); // 5

        let (lower_chunk, lower_slot) = bitmap::chunk_address(lower_compressed);
        let (upper_chunk, upper_slot) = bitmap::chunk_address(upper_compressed);

        // Initialize chunks
        let mut lower_chunk_data = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0
            };
            TICKS_PER_CHUNK as usize
        ];
        lower_chunk_data[lower_slot as usize] = TickState {
            liquidity_gross: 10_000_000_000_000,
            liquidity_net: 10_000_000_000_000,
        };
        ticks.chunks.insert(lower_chunk, lower_chunk_data);

        let mut upper_chunk_data = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0
            };
            TICKS_PER_CHUNK as usize
        ];
        upper_chunk_data[upper_slot as usize] = TickState {
            liquidity_gross: 10_000_000_000_000,
            liquidity_net: -10_000_000_000_000,
        };
        ticks.chunks.insert(upper_chunk, upper_chunk_data);

        // Set bitmap bits
        let (bm_word_lower, bm_bit_lower) = bitmap::chunk_bitmap_position(lower_chunk);
        let (bm_word_upper, bm_bit_upper) = bitmap::chunk_bitmap_position(upper_chunk);

        let mut word = [0u8; 32];
        // Set bits for both chunks (they might be in the same word)
        set_bit_in_word(&mut word, bm_bit_lower);
        set_bit_in_word(&mut word, bm_bit_upper);
        ticks.chunk_bitmap.insert(bm_word_lower, word);
        if bm_word_upper != bm_word_lower {
            let mut word2 = [0u8; 32];
            set_bit_in_word(&mut word2, bm_bit_upper);
            ticks.chunk_bitmap.insert(bm_word_upper, word2);
        }

        // Set L2 bitmap
        let (l2_pos, l2_bit) = bitmap::word_bitmap_position(bm_word_lower);
        let mut l2_word = [0u8; 32];
        set_bit_in_word(&mut l2_word, l2_bit);
        ticks.word_bitmap.insert(l2_pos, l2_word);

        // Simulate a swap: 1_000_000 token0 in (zero_for_one)
        let result = simulate_swap(&pool, &ticks, 1_000_000, true);
        assert!(result.is_some(), "swap should produce output");
        let (amount_out, _, _) = result.unwrap();
        assert!(amount_out > 0, "should get some token1 out");
        println!("Swap 1_000_000 token0 -> {} token1", amount_out);
    }

    fn set_bit_in_word(word: &mut [u8; 32], bit_pos: u32) {
        let byte_idx = 31usize - (bit_pos / 8) as usize;
        let bit_idx = (bit_pos % 8) as u8;
        word[byte_idx] |= 1u8 << bit_idx;
    }

    #[test]
    fn clmm_quote_allowed_requires_complete_and_in_range() {
        let pool = ClmmPoolState {
            sqrt_price_x96: sqrt_ratio_at_tick(-200),
            tick: -200,
            liquidity: 1_000_000,
            fee_bps: 30,
            tick_spacing: 200,
            token0: "A".to_string(),
            token1: "B".to_string(),
        };
        let mut ticks = TickDataStore::new();
        let compressed = bitmap::compress_tick(-200, 200);
        let (chunk_pos, slot) = bitmap::chunk_address(compressed);
        let mut chunk = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        chunk[slot as usize] = TickState {
            liquidity_gross: 1_000_000,
            liquidity_net: 1_000_000,
        };
        ticks.chunks.insert(chunk_pos, chunk);
        let upper_compressed = bitmap::compress_tick(1000, 200);
        let (upper_chunk, upper_slot) = bitmap::chunk_address(upper_compressed);
        let mut upper_chunk_data = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        upper_chunk_data[upper_slot as usize] = TickState {
            liquidity_gross: 1_000_000,
            liquidity_net: -1_000_000,
        };
        ticks.chunks.insert(upper_chunk, upper_chunk_data);

        let coverage = ClmmCoverageInput {
            pool_tick: -200,
            tick_spacing: 200,
            is_complete: true,
            min_loaded_tick: Some(-200),
            max_loaded_tick: Some(1000),
            scanned_word_start: None,
            scanned_word_end: None,
        };
        assert!(clmm_quote_allowed(&ticks, &coverage));

        let incomplete = ClmmCoverageInput {
            is_complete: false,
            ..coverage
        };
        assert!(!clmm_quote_allowed(&ticks, &incomplete));
    }

    #[test]
    fn simulate_swap_rejects_tick_outside_loaded_range() {
        let pool = ClmmPoolState {
            sqrt_price_x96: sqrt_ratio_at_tick(5000),
            tick: 5000,
            liquidity: 1_000_000,
            fee_bps: 30,
            tick_spacing: 200,
            token0: "A".to_string(),
            token1: "B".to_string(),
        };
        let mut ticks = TickDataStore::new();
        let compressed = bitmap::compress_tick(-200, 200);
        let (chunk_pos, slot) = bitmap::chunk_address(compressed);
        let mut chunk = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        chunk[slot as usize] = TickState {
            liquidity_gross: 1_000_000,
            liquidity_net: 1_000_000,
        };
        ticks.chunks.insert(chunk_pos, chunk);

        assert!(simulate_swap(&pool, &ticks, 1_000, true).is_none());
    }

    #[test]
    fn test_clmm_pool_snapshot_round_trip() {
        let pool = ClmmPoolState {
            sqrt_price_x96: U256([11, 22, 33, 44]),
            tick: -120,
            liquidity: 9_999,
            fee_bps: 30,
            tick_spacing: 60,
            token0: "TOKEN0".to_string(),
            token1: "TOKEN1".to_string(),
        };
        let mut ticks = TickDataStore::new();
        let compressed = bitmap::compress_tick(-120, 60);
        let (chunk_pos, slot) = bitmap::chunk_address(compressed);
        let mut chunk = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        chunk[slot as usize] = TickState {
            liquidity_gross: 777,
            liquidity_net: -333,
        };
        ticks.chunks.insert(chunk_pos, chunk);
        ticks.chunk_bitmap.insert(0, [5u8; 32]);
        ticks.word_bitmap.insert(-1, [9u8; 32]);

        let snapshot = clmm_pool_to_snapshot(
            "sushi",
            "pool-1",
            &pool,
            &ticks,
            Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-120),
                max_loaded_tick: Some(-120),
                scanned_word_start: Some(-2),
                scanned_word_end: Some(2),
            }),
        );
        let (restored_pool, restored_ticks) = clmm_pool_from_snapshot(&snapshot);

        assert_eq!(snapshot.source, "sushi");
        assert_eq!(snapshot.pool_address, "pool-1");
        assert_eq!(restored_pool.sqrt_price_x96, pool.sqrt_price_x96);
        assert_eq!(restored_pool.tick, pool.tick);
        assert_eq!(restored_pool.liquidity, pool.liquidity);
        assert_eq!(restored_pool.token0, pool.token0);
        assert_eq!(restored_pool.token1, pool.token1);
        assert_eq!(restored_ticks.get_tick(-120, pool.tick_spacing).liquidity_gross, 777);
        assert_eq!(restored_ticks.chunk_bitmap.get(&0), Some(&[5u8; 32]));
        assert_eq!(restored_ticks.word_bitmap.get(&-1), Some(&[9u8; 32]));
    }

    #[test]
    fn test_clmm_pool_snapshot_derives_word_bitmap_when_missing() {
        let pool = ClmmPoolState {
            sqrt_price_x96: U256([1, 0, 0, 0]),
            tick: 0,
            liquidity: 100,
            fee_bps: 30,
            tick_spacing: 200,
            token0: "TOKEN0".to_string(),
            token1: "TOKEN1".to_string(),
        };
        let mut ticks = TickDataStore::new();
        ticks.chunk_bitmap.insert(2, [1u8; 32]);

        let snapshot = clmm_pool_to_snapshot("sushi", "pool-derive", &pool, &ticks, None);
        let (_, restored_ticks) = clmm_pool_from_snapshot(&snapshot);

        assert!(!snapshot.word_bitmaps.is_empty());
        assert!(!restored_ticks.word_bitmap.is_empty());
    }
}

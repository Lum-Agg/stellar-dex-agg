//! Curve StableSwap math — pure Rust port for N-token pools.
//!
//! Implements the Curve invariant: A*n^n * sum(x_i) + D = A*n^n*D +
//! D^(n+1)/(n^n * prod(x_i)) Used by Aquarius stableswap pools (2-token and
//! 3-token).
//!
//! All calculations use u128 with iterative Newton's method (same as on-chain).

/// Fee denominator (10000 = 100%)
pub const FEE_DENOMINATOR: u128 = 10_000;

fn mul_div(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    a.checked_mul(b).and_then(|v| v.checked_div(d))
}

/// Pool state for stableswap computation.
#[derive(Debug, Clone)]
pub struct StablePool {
    /// Reserves for each token (in native decimals)
    pub reserves: Vec<u128>,
    /// Decimals for each token
    pub decimals: Vec<u32>,
    /// Amplification coefficient (A)
    pub amp: u128,
    /// Fee in basis points (e.g., 30 = 0.3%)
    pub fee_bps: u32,
}

impl StablePool {
    /// Compute precision_mul for each token (scales to max decimals).
    fn precision_mul(&self) -> Vec<u128> {
        let max_dec = self.decimals.iter().copied().max().unwrap_or(7);
        self.decimals.iter().map(|&d| 10u128.pow(max_dec - d)).collect()
    }

    /// Normalize reserves to common precision (xp).
    fn xp(&self) -> Vec<u128> {
        let pmul = self.precision_mul();
        self.reserves
            .iter()
            .zip(pmul.iter())
            .filter_map(|(&r, &m)| r.checked_mul(m))
            .collect()
    }

    /// Compute the StableSwap invariant D.
    /// Uses Newton's method, converges in ~4 iterations.
    fn get_d(&self, xp: &[u128]) -> u128 {
        let n = xp.len() as u128;
        let s: u128 = xp.iter().sum();
        if s == 0 {
            return 0;
        }

        let ann = self.amp * n; // A * n^n (for n=2: A*4, for n=3: A*27)

        let mut d = s;
        for _ in 0..255 {
            // d_p = D^(n+1) / (n^n * prod(x_i))
            let mut d_p = d;
            for &x in xp {
                if x == 0 {
                    return 0;
                }
                d_p = mul_div(d_p, d, x.saturating_mul(n)).unwrap_or(u128::MAX);
            }

            let d_prev = d;
            // Newton step: d = (ann*s + d_p*n) * d / ((ann-1)*d + (n+1)*d_p)
            let numerator = ann
                .saturating_mul(s)
                .saturating_add(d_p.saturating_mul(n))
                .saturating_mul(d);
            let denominator = ann
                .saturating_sub(1)
                .saturating_mul(d)
                .saturating_add((n + 1).saturating_mul(d_p));
            if denominator == 0 {
                break;
            }
            d = numerator / denominator;

            if d.abs_diff(d_prev) <= 1 {
                return d;
            }
        }
        d
    }

    /// Given new balance x for token[in_idx], find balance y for token[out_idx]
    /// that satisfies the invariant.
    fn get_y(&self, in_idx: usize, out_idx: usize, x: u128, xp: &[u128]) -> u128 {
        let n = xp.len() as u128;
        let d = self.get_d(xp);
        let ann = self.amp * n;

        let mut c = d;
        let mut s: u128 = 0;

        for i in 0..xp.len() {
            let x_i = if i == in_idx {
                x
            } else if i != out_idx {
                xp[i]
            } else {
                continue;
            };
            s = s.saturating_add(x_i);
            if x_i == 0 {
                return 0;
            }
            c = mul_div(c, d, x_i.saturating_mul(n)).unwrap_or(0);
        }

        c = mul_div(c, d, ann.saturating_mul(n)).unwrap_or(0);
        let b = s + d / ann; // note: b > d always (since s >= d/ann is not guaranteed, but in practice it
                             // is)

        // Newton's method to find y
        let mut y = d;
        for _ in 0..255 {
            let y_prev = y;
            // y = (y^2 + c) / (2*y + b - d)
            let numerator = y.saturating_mul(y).saturating_add(c);
            let denominator = 2 * y + b - d;
            if denominator == 0 {
                break;
            }
            y = numerator / denominator;

            if y.abs_diff(y_prev) <= 1 {
                return y;
            }
        }
        y
    }

    /// Calculate output amount for swapping `dx` of token[i] to token[j].
    /// This is the main function used for quoting.
    pub fn get_dy(&self, i: usize, j: usize, dx: u128) -> u128 {
        if i == j || i >= self.reserves.len() || j >= self.reserves.len() {
            return 0;
        }
        if dx == 0 {
            return 0;
        }

        let pmul = self.precision_mul();
        let xp = self.xp();
        if xp.len() != self.reserves.len() {
            return 0;
        }

        // Apply fee to input
        let dx_fee = (dx as u128 * self.fee_bps as u128 + FEE_DENOMINATOR - 1) / FEE_DENOMINATOR;
        let dx_after_fee = dx - dx_fee;

        // Scale dx to common precision
        let scaled_in = dx_after_fee.checked_mul(pmul[i]).unwrap_or(0);
        let x = xp[i].saturating_add(scaled_in);

        // Find new y
        let y = self.get_y(i, j, x, &xp);

        if y == 0 || xp[j] <= y {
            return 0;
        }

        // Output in native decimals
        let dy = (xp[j] - y - 1) / pmul[j];
        dy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_2token_equal() {
        // 2-token pool with equal reserves (like USDC/EURC)
        let pool = StablePool {
            reserves: vec![1_000_0000000, 1_000_0000000], // 1000 each, 7 decimals
            decimals: vec![7, 7],
            amp: 1500,
            fee_bps: 30,
        };

        // Swap 10 token0 -> token1
        let out = pool.get_dy(0, 1, 10_0000000);
        // Should be close to 10 (minus fee) for a stable pool
        assert!(out > 9_9000000, "out={}, expected ~9.97", out);
        assert!(out < 10_0000000, "out={}, should be less than input", out);
    }

    #[test]
    fn test_stable_3token_router_fixture() {
        // Same fixture as aquarius::three_token_stableswap_matches_router_test_vector
        let pool = StablePool {
            reserves: vec![100_0000000, 100_0000000, 100_0000000],
            decimals: vec![7, 7, 7],
            amp: 6750,
            fee_bps: 30,
        };
        let out = pool.get_dy(0, 1, 97_0000000);
        assert_eq!(out, 96_5081326);
    }

    #[test]
    fn test_stable_3token() {
        // 3-token pool (like XLM/USDC/AQUA)
        let pool = StablePool {
            reserves: vec![329_248_0000000, 7_225_0000000, 1_776_417_0000000],
            decimals: vec![7, 7, 7],
            amp: 100,
            fee_bps: 30,
        };

        // Swap 1 USDC (token1) -> XLM (token0)
        let out = pool.get_dy(1, 0, 1_0000000);
        println!("1 USDC -> {} XLM (stroops)", out);
        // With these reserves, 1 USDC should get roughly 1 XLM worth
        assert!(out > 0, "should get some output");
    }

    #[test]
    fn test_stable_zero_input() {
        let pool = StablePool {
            reserves: vec![1000_0000000, 1000_0000000],
            decimals: vec![7, 7],
            amp: 1500,
            fee_bps: 30,
        };
        assert_eq!(pool.get_dy(0, 1, 0), 0);
    }

    #[test]
    fn test_stable_same_token() {
        let pool = StablePool {
            reserves: vec![1000_0000000, 1000_0000000],
            decimals: vec![7, 7],
            amp: 1500,
            fee_bps: 30,
        };
        assert_eq!(pool.get_dy(0, 0, 10_0000000), 0);
    }
}

//! Split optimizer: determines optimal allocation of input across multiple paths.
//!
//! Uses Brent's method for 2-path optimization (finds optimal split ratio with
//! ~10 evaluations to 0.01% precision), and recursive pairwise optimization for
//! N paths.
//!
//! Algorithm:
//! 1. If best single path has price impact < threshold, use it directly.
//! 2. For 2 paths: use Brent's method to find optimal ratio x in [0, 1].
//! 3. For N paths: recursively merge paths pairwise, optimizing each merge.
//!
//! Reference: Jupiter's Iris engine uses Golden-section + Brent's method.

use crate::types::{OptimalRoute, Path, Quote, SubOrder};
use tracing::debug;

/// Configuration for split optimization.
#[derive(Debug, Clone)]
pub struct SplitConfig {
    /// Price impact threshold (bps) above which splitting is considered.
    pub split_threshold_bps: u32,
    /// Maximum number of splits.
    pub max_splits: usize,
    /// Brent's method tolerance (fraction, e.g., 0.0001 = 0.01%)
    pub tolerance: f64,
    /// Maximum iterations for Brent's method
    pub max_iterations: usize,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            split_threshold_bps: 10, // 0.1% - split when impact exceeds this
            max_splits: 5,
            tolerance: 0.0001, // 0.01% precision
            max_iterations: 50,
        }
    }
}

/// Quoted path: a path with its quote at a specific amount.
#[derive(Debug, Clone)]
pub struct QuotedPath {
    pub path: Path,
    pub quote: Quote,
}

pub struct SplitOptimizer {
    config: SplitConfig,
}

impl SplitOptimizer {
    pub fn new(config: SplitConfig) -> Self {
        Self { config }
    }

    /// Determine optimal split and compute the best route.
    ///
    /// `quoted_paths`: paths with quotes at the full input amount (used to rank them).
    /// `quote_fn`: function to get output for a path at a specific input amount.
    pub async fn optimize<F, Fut>(
        &self,
        quoted_paths: &[QuotedPath],
        total_amount: u128,
        slippage_bps: u32,
        quote_fn: F,
    ) -> OptimalRoute
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        let start = std::time::Instant::now();

        if quoted_paths.is_empty() {
            return empty_route(total_amount, start.elapsed().as_millis() as u64);
        }

        // Sort by output (best first)
        let mut sorted: Vec<&QuotedPath> = quoted_paths.iter().collect();
        sorted.sort_by(|a, b| b.quote.amount_out.cmp(&a.quote.amount_out));

        let best_single = sorted[0];
        let best_single_out = best_single.quote.amount_out;
        let best_single_impact = best_single.quote.price_impact_bps;

        // If price impact is below threshold or only 1 path, no split needed
        if best_single_impact < self.config.split_threshold_bps || sorted.len() < 2 {
            let minimum_out = apply_slippage(best_single_out, slippage_bps);
            return OptimalRoute {
                sub_orders: vec![SubOrder {
                    path: best_single.path.clone(),
                    amount_in: total_amount,
                    expected_amount_out: best_single_out,
                    fraction: 1.0,
                }],
                total_amount_in: total_amount,
                total_expected_out: best_single_out,
                price_impact_bps: best_single_impact,
                is_split: false,
                improvement_bps: 0,
                minimum_out,
                compute_time_ms: start.elapsed().as_millis() as u64,
            };
        }

        // Take top N paths for split optimization
        let candidates: Vec<&QuotedPath> =
            sorted.into_iter().take(self.config.max_splits).collect();

        // Optimize split using recursive pairwise Brent's method
        let split_result = self
            .optimize_n_paths(&candidates, total_amount, &quote_fn)
            .await;

        let total_out: u128 = split_result.iter().map(|(_, out)| out).sum();

        // Check if split is actually better
        if total_out <= best_single_out {
            let minimum_out = apply_slippage(best_single_out, slippage_bps);
            return OptimalRoute {
                sub_orders: vec![SubOrder {
                    path: best_single.path.clone(),
                    amount_in: total_amount,
                    expected_amount_out: best_single_out,
                    fraction: 1.0,
                }],
                total_amount_in: total_amount,
                total_expected_out: best_single_out,
                price_impact_bps: best_single_impact,
                is_split: false,
                improvement_bps: 0,
                minimum_out,
                compute_time_ms: start.elapsed().as_millis() as u64,
            };
        }

        // Build sub-orders from split result
        let sub_orders: Vec<SubOrder> = split_result
            .iter()
            .filter(|(amount, _)| *amount > 0)
            .enumerate()
            .map(|(i, (amount, out))| SubOrder {
                path: candidates[i].path.clone(),
                amount_in: *amount,
                expected_amount_out: *out,
                fraction: *amount as f64 / total_amount as f64,
            })
            .collect();

        let improvement_bps = ((total_out - best_single_out) * 10_000 / best_single_out) as u32;
        let minimum_out = apply_slippage(total_out, slippage_bps);
        let compute_time_ms = start.elapsed().as_millis() as u64;

        debug!(
            total_out,
            best_single_out,
            improvement_bps,
            splits = sub_orders.len(),
            compute_time_ms,
            "Split optimization complete (Brent's method)"
        );

        OptimalRoute {
            sub_orders,
            total_amount_in: total_amount,
            total_expected_out: total_out,
            price_impact_bps: best_single_impact / 2, // rough estimate
            is_split: true,
            improvement_bps,
            minimum_out,
            compute_time_ms,
        }
    }

    /// Optimize N paths using recursive pairwise Brent's method.
    ///
    /// Strategy: for N paths, we recursively find the optimal split between
    /// "path 0" and "the rest combined". Then recurse on "the rest".
    async fn optimize_n_paths<F, Fut>(
        &self,
        paths: &[&QuotedPath],
        total_amount: u128,
        quote_fn: &F,
    ) -> Vec<(u128, u128)>
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        if paths.len() == 1 {
            let out = quote_fn(&paths[0].path, total_amount)
                .await
                .map(|q| q.amount_out)
                .unwrap_or(0);
            return vec![(total_amount, out)];
        }

        if paths.len() == 2 {
            return self
                .optimize_two_paths(&paths[0].path, &paths[1].path, total_amount, quote_fn)
                .await;
        }

        // For N > 2: find optimal split between path[0] and paths[1..] combined
        let path_a = &paths[0].path;
        let rest = &paths[1..];

        // Define the objective: given fraction x to path_a, what's the total output?
        // We use Brent's method to maximize f(x) = output_a(x * total) + output_rest((1-x) * total)
        let optimal_fraction = self
            .brent_maximize(0.0, 1.0, |x| {
                let amount_a = (x * total_amount as f64) as u128;
                let amount_rest = total_amount.saturating_sub(amount_a);

                // For "rest combined", use the best single path among rest as approximation
                // (full recursive would be too expensive in async context)
                let path_a_clone = path_a.clone();
                let rest_best = rest[0].path.clone();

                async move {
                    let out_a = quote_fn(&path_a_clone, amount_a)
                        .await
                        .map(|q| q.amount_out)
                        .unwrap_or(0);
                    let out_rest = quote_fn(&rest_best, amount_rest)
                        .await
                        .map(|q| q.amount_out)
                        .unwrap_or(0);
                    (out_a + out_rest) as f64
                }
            })
            .await;

        let amount_a = (optimal_fraction * total_amount as f64) as u128;
        let amount_rest = total_amount.saturating_sub(amount_a);

        let out_a = quote_fn(path_a, amount_a)
            .await
            .map(|q| q.amount_out)
            .unwrap_or(0);

        let mut result = vec![(amount_a, out_a)];

        // Recurse on the rest
        if amount_rest > 0 && rest.len() > 0 {
            let rest_results = Box::pin(self.optimize_n_paths(rest, amount_rest, quote_fn)).await;
            result.extend(rest_results);
        }

        result
    }

    /// Optimize split between exactly 2 paths using Brent's method.
    async fn optimize_two_paths<F, Fut>(
        &self,
        path_a: &Path,
        path_b: &Path,
        total_amount: u128,
        quote_fn: &F,
    ) -> Vec<(u128, u128)>
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        // Find optimal x in [0, 1] where x = fraction to path_a
        let path_a_clone = path_a.clone();
        let path_b_clone = path_b.clone();

        let optimal_x = self
            .brent_maximize(0.0, 1.0, |x| {
                let amount_a = (x * total_amount as f64) as u128;
                let amount_b = total_amount.saturating_sub(amount_a);
                let pa = path_a_clone.clone();
                let pb = path_b_clone.clone();

                async move {
                    let out_a = if amount_a > 0 {
                        quote_fn(&pa, amount_a)
                            .await
                            .map(|q| q.amount_out)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let out_b = if amount_b > 0 {
                        quote_fn(&pb, amount_b)
                            .await
                            .map(|q| q.amount_out)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    (out_a + out_b) as f64
                }
            })
            .await;

        let amount_a = (optimal_x * total_amount as f64) as u128;
        let amount_b = total_amount.saturating_sub(amount_a);

        let out_a = if amount_a > 0 {
            quote_fn(path_a, amount_a)
                .await
                .map(|q| q.amount_out)
                .unwrap_or(0)
        } else {
            0
        };
        let out_b = if amount_b > 0 {
            quote_fn(path_b, amount_b)
                .await
                .map(|q| q.amount_out)
                .unwrap_or(0)
        } else {
            0
        };

        vec![(amount_a, out_a), (amount_b, out_b)]
    }

    /// Brent's method for maximizing a unimodal function on [a, b].
    ///
    /// Combines golden-section search with parabolic interpolation for
    /// superlinear convergence. Typically finds optimum in ~10 evaluations.
    ///
    /// We maximize by negating (Brent's finds minimum, we want maximum).
    async fn brent_maximize<F, Fut>(&self, a: f64, b: f64, f: F) -> f64
    where
        F: Fn(f64) -> Fut,
        Fut: std::future::Future<Output = f64>,
    {
        let golden = 0.381966011250105; // (3 - sqrt(5)) / 2
        let tol = self.config.tolerance;
        let max_iter = self.config.max_iterations;

        let mut a = a;
        let mut b = b;
        let mut x = a + golden * (b - a);
        let mut w = x;
        let mut v = x;
        let mut fx = -f(x).await; // negate for minimization
        let mut fw = fx;
        let mut fv = fx;
        let mut d = 0.0_f64;
        let mut e = 0.0_f64;

        for _ in 0..max_iter {
            let midpoint = 0.5 * (a + b);
            let tol1 = tol * x.abs() + 1e-10;
            let tol2 = 2.0 * tol1;

            // Check convergence
            if (x - midpoint).abs() <= tol2 - 0.5 * (b - a) {
                break;
            }

            // Try parabolic interpolation
            let mut use_golden = true;
            if e.abs() > tol1 {
                // Fit parabola through x, w, v
                let r = (x - w) * (fx - fv);
                let q = (x - v) * (fx - fw);
                let p = (x - v) * q - (x - w) * r;
                let q = 2.0 * (q - r);
                let (p, q) = if q > 0.0 { (-p, q) } else { (p, -q) };

                // Accept parabolic step if it's within bounds
                if p.abs() < (0.5 * q * e).abs() && p > q * (a - x) && p < q * (b - x) {
                    d = p / q;
                    let u = x + d;
                    if (u - a) < tol2 || (b - u) < tol2 {
                        d = if x < midpoint { tol1 } else { -tol1 };
                    }
                    use_golden = false;
                }
            }

            if use_golden {
                e = if x < midpoint { b - x } else { a - x };
                d = golden * e;
            } else {
                e = d;
            }

            // Evaluate at new point
            let u = if d.abs() >= tol1 {
                x + d
            } else if d > 0.0 {
                x + tol1
            } else {
                x - tol1
            };

            let fu = -f(u).await; // negate for minimization

            // Update brackets
            if fu <= fx {
                if u < x {
                    b = x;
                } else {
                    a = x;
                }
                v = w;
                fv = fw;
                w = x;
                fw = fx;
                x = u;
                fx = fu;
            } else {
                if u < x {
                    a = u;
                } else {
                    b = u;
                }
                if fu <= fw || w == x {
                    v = w;
                    fv = fw;
                    w = u;
                    fw = fu;
                } else if fu <= fv || v == x || v == w {
                    v = u;
                    fv = fu;
                }
            }
        }

        x.clamp(0.0, 1.0)
    }
}

fn apply_slippage(amount: u128, slippage_bps: u32) -> u128 {
    amount * (10_000 - slippage_bps as u128) / 10_000
}

fn empty_route(total_amount: u128, compute_time_ms: u64) -> OptimalRoute {
    OptimalRoute {
        sub_orders: vec![],
        total_amount_in: total_amount,
        total_expected_out: 0,
        price_impact_bps: 0,
        is_split: false,
        improvement_bps: 0,
        minimum_out: 0,
        compute_time_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Brent's method on a simple quadratic (maximum at x=0.6)
    #[tokio::test]
    async fn test_brent_quadratic() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());

        // f(x) = -(x - 0.6)^2 + 1, maximum at x = 0.6
        let result = optimizer
            .brent_maximize(0.0, 1.0, |x| async move { -(x - 0.6) * (x - 0.6) + 1.0 })
            .await;

        assert!(
            (result - 0.6).abs() < 0.001,
            "Expected ~0.6, got {}",
            result
        );
    }

    /// Test Brent's method on AMM-like diminishing returns
    #[tokio::test]
    async fn test_brent_amm_split() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());

        // Simulate two AMM pools with different depths
        // Pool A: reserve 1000, Pool B: reserve 500
        // Total input: 100
        // Optimal split should put more into Pool A (deeper)
        let total = 100.0;
        let result = optimizer
            .brent_maximize(0.0, 1.0, |x| async move {
                let amount_a = x * total;
                let amount_b = (1.0 - x) * total;
                // xy=k output: amount * reserve / (reserve + amount)
                let out_a = amount_a * 1000.0 / (1000.0 + amount_a);
                let out_b = amount_b * 500.0 / (500.0 + amount_b);
                out_a + out_b
            })
            .await;

        // Pool A is deeper, so optimal split should favor A (x > 0.5)
        assert!(
            result > 0.5,
            "Expected x > 0.5 (favor deeper pool), got {}",
            result
        );
        assert!(
            result < 0.8,
            "Expected x < 0.8 (still use both), got {}",
            result
        );
    }

    /// Test that 100% to one pool is chosen when other pool is empty
    #[tokio::test]
    async fn test_brent_one_pool_dominant() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());

        let result = optimizer
            .brent_maximize(0.0, 1.0, |x| async move {
                let amount_a = x * 100.0;
                // Pool A: deep liquidity
                let out_a = amount_a * 10000.0 / (10000.0 + amount_a);
                // Pool B: zero liquidity
                let out_b = 0.0;
                out_a + out_b
            })
            .await;

        // Should put everything in Pool A
        assert!(result > 0.99, "Expected ~1.0, got {}", result);
    }
}

//! Split optimizer: determines optimal allocation of input across multiple
//! paths.
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

use {
    crate::types::{OptimalRoute, Path, Quote, RouteDebug, RouteDebugCandidate, RouteDebugPlannedSplit, SubOrder},
    tracing::debug,
};

/// Configuration for split optimization.
#[derive(Debug, Clone)]
pub struct SplitConfig {
    /// Price impact threshold (bps) above which splitting is considered.
    pub split_threshold_bps: u32,
    /// If the second-best path is within this delta of the best path, still
    /// attempt split.
    pub split_competitive_delta_bps: u32,
    /// Drop split legs whose expected output is below this share of total
    /// output.
    pub min_split_fraction_bps: u32,
    /// Drop split legs whose input is below this share of total input (prevents
    /// dust legs).
    pub min_split_amount_in_bps: u32,
    /// Reject a leg when its out/in ratio deviates from the path's full-size
    /// quote by more than this.
    pub max_leg_rate_deviation_bps: u32,
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
            split_threshold_bps: 5,          // 0.05% - split when impact exceeds this
            split_competitive_delta_bps: 50, // 0.5% output gap still worth checking
            min_split_fraction_bps: 5,       // 0.05% minimum share of total output
            min_split_amount_in_bps: 10,     // 0.10% minimum share of total input
            max_leg_rate_deviation_bps: 500, // 5% vs full-size quote for that path
            max_splits: 5,
            tolerance: 0.0001, // 0.01% precision
            max_iterations: 18,
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

    pub fn config(&self) -> &SplitConfig {
        &self.config
    }

    /// Determine optimal split and compute the best route.
    ///
    /// `quoted_paths`: paths with quotes at the full input amount (used to rank
    /// them). `quote_fn`: function to get output for a path at a specific
    /// input amount.
    pub async fn optimize<F, Fut>(
        &self,
        quoted_paths: &[QuotedPath],
        total_amount: u128,
        slippage_bps: u32,
        max_splits_override: Option<usize>,
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
        let second_best_out = sorted.get(1).map(|p| p.quote.amount_out);
        let competitive_delta_bps = self.config.split_competitive_delta_bps;
        let competitive_enough = second_best_out
            .map(|second| {
                let gap_bps = if best_single_out == 0 {
                    u32::MAX
                } else {
                    (((best_single_out.saturating_sub(second)) * 10_000) / best_single_out) as u32
                };
                gap_bps <= competitive_delta_bps
            })
            .unwrap_or(false);

        let max_splits = max_splits_override.unwrap_or(self.config.max_splits);

        if max_splits <= 1 {
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
                debug: Some(RouteDebug {
                    quoted_paths_count: quoted_paths.len(),
                    candidate_paths_count: 1,
                    best_single_out,
                    second_best_out,
                    best_single_impact_bps: best_single_impact,
                    split_threshold_bps: self.config.split_threshold_bps,
                    competitive_delta_bps,
                    min_split_fraction_bps: self.config.min_split_fraction_bps,
                    split_attempted: false,
                    split_rejected_reason: Some("max_splits_1".to_string()),
                    optimization_strategy: "single_path_only".to_string(),
                    used_rest_best_approximation: false,
                    split_total_out: None,
                    dust_filtered_legs: 0,
                    candidate_routes: vec![],
                    planned_split: vec![],
                }),
            };
        }

        if sorted.len() < 2 {
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
                debug: Some(RouteDebug {
                    quoted_paths_count: quoted_paths.len(),
                    candidate_paths_count: 1,
                    best_single_out,
                    second_best_out,
                    best_single_impact_bps: best_single_impact,
                    split_threshold_bps: self.config.split_threshold_bps,
                    competitive_delta_bps,
                    min_split_fraction_bps: self.config.min_split_fraction_bps,
                    split_attempted: false,
                    split_rejected_reason: Some("not_enough_paths".to_string()),
                    optimization_strategy: "not_applicable".to_string(),
                    used_rest_best_approximation: false,
                    split_total_out: None,
                    dust_filtered_legs: 0,
                    candidate_routes: vec![],
                    planned_split: vec![],
                }),
            };
        }

        // Competitive second path only triggers split when there is measurable impact.
        // Otherwise XLM/USDC-style pairs (impact ≈ 0, many similar paths) run Brent for
        // ~10s+ with no gain.
        let competitive_split = competitive_enough && best_single_impact > 0;
        if best_single_impact < self.config.split_threshold_bps && !competitive_split {
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
                debug: Some(RouteDebug {
                    quoted_paths_count: quoted_paths.len(),
                    candidate_paths_count: sorted.len(),
                    best_single_out,
                    second_best_out,
                    best_single_impact_bps: best_single_impact,
                    split_threshold_bps: self.config.split_threshold_bps,
                    competitive_delta_bps,
                    min_split_fraction_bps: self.config.min_split_fraction_bps,
                    split_attempted: false,
                    split_rejected_reason: Some("below_threshold_and_not_competitive".to_string()),
                    optimization_strategy: "not_attempted".to_string(),
                    used_rest_best_approximation: false,
                    split_total_out: None,
                    dust_filtered_legs: 0,
                    candidate_routes: vec![],
                    planned_split: vec![],
                }),
            };
        }

        let candidates: Vec<&QuotedPath> = sorted.into_iter().take(max_splits).collect();
        let candidate_paths_count = candidates.len();
        let optimization_strategy = if candidate_paths_count > 2 {
            "recursive_pairwise_approx_rest".to_string()
        } else {
            "two_path_brent".to_string()
        };
        let used_rest_best_approximation = candidate_paths_count > 2;
        let candidate_routes = build_candidate_debug(&candidates);

        // Optimize split using recursive pairwise Brent's method
        let split_result = self.optimize_n_paths(&candidates, total_amount, &quote_fn).await;
        let (effective_candidate_indices, split_result, dust_filtered_legs) = self
            .filter_dust_split_legs(&candidates, split_result, total_amount, &quote_fn)
            .await;
        let effective_candidates: Vec<&QuotedPath> =
            effective_candidate_indices.iter().map(|&idx| candidates[idx]).collect();
        let planned_split = build_planned_split_debug(&effective_candidates, &split_result, total_amount);

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
                debug: Some(RouteDebug {
                    quoted_paths_count: quoted_paths.len(),
                    candidate_paths_count,
                    best_single_out,
                    second_best_out,
                    best_single_impact_bps: best_single_impact,
                    split_threshold_bps: self.config.split_threshold_bps,
                    competitive_delta_bps,
                    min_split_fraction_bps: self.config.min_split_fraction_bps,
                    split_attempted: true,
                    split_rejected_reason: Some("no_improvement".to_string()),
                    optimization_strategy,
                    used_rest_best_approximation,
                    split_total_out: Some(total_out),
                    dust_filtered_legs,
                    candidate_routes,
                    planned_split,
                }),
            };
        }

        // Build sub-orders from split result
        let sub_orders: Vec<SubOrder> = split_result
            .iter()
            .filter(|(amount, _)| *amount > 0)
            .enumerate()
            .map(|(i, (amount, out))| SubOrder {
                path: effective_candidates[i].path.clone(),
                amount_in: *amount,
                expected_amount_out: *out,
                fraction: *amount as f64 / total_amount as f64,
            })
            .collect();

        let improvement_bps = ((total_out - best_single_out) * 10_000 / best_single_out) as u32;
        let minimum_out = apply_split_minimum_slippage(total_out, slippage_bps);
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
            debug: Some(RouteDebug {
                quoted_paths_count: quoted_paths.len(),
                candidate_paths_count,
                best_single_out,
                second_best_out,
                best_single_impact_bps: best_single_impact,
                split_threshold_bps: self.config.split_threshold_bps,
                competitive_delta_bps,
                min_split_fraction_bps: self.config.min_split_fraction_bps,
                split_attempted: true,
                split_rejected_reason: None,
                optimization_strategy,
                used_rest_best_approximation,
                split_total_out: Some(total_out),
                dust_filtered_legs,
                candidate_routes,
                planned_split,
            }),
        }
    }

    async fn filter_dust_split_legs<F, Fut>(
        &self,
        candidates: &[&QuotedPath],
        split_result: Vec<(u128, u128)>,
        total_amount: u128,
        quote_fn: &F,
    ) -> (Vec<usize>, Vec<(u128, u128)>, usize)
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        if self.config.min_split_fraction_bps == 0 || candidates.len() < 2 {
            return ((0..candidates.len()).collect(), split_result, 0);
        }

        let mut active_indices: Vec<usize> = (0..candidates.len()).collect();
        let mut current_split = split_result;
        let mut dust_filtered_legs = 0usize;

        loop {
            let total_out: u128 = current_split.iter().map(|(_, out)| *out).sum();
            if total_out == 0 || active_indices.len() < 2 {
                break;
            }

            let kept_positions: Vec<usize> = current_split
                .iter()
                .enumerate()
                .filter(|(position, (amount, out))| {
                    if *amount == 0 {
                        return false;
                    }
                    let candidate = active_indices[*position];
                    let amount_in_bps = split_amount_in_fraction_bps(*amount, total_amount);
                    let out_bps = (*out * 10_000 / total_out) as u32;
                    amount_in_bps >= self.config.min_split_amount_in_bps &&
                        out_bps >= self.config.min_split_fraction_bps &&
                        leg_rate_matches_full_quote(
                            candidates[candidate],
                            *amount,
                            *out,
                            self.config.max_leg_rate_deviation_bps,
                        )
                })
                .map(|(idx, _)| idx)
                .collect();

            let active_nonzero = current_split.iter().filter(|(amount, _)| *amount > 0).count();
            if kept_positions.len() == active_nonzero || kept_positions.is_empty() {
                break;
            }

            dust_filtered_legs += active_nonzero - kept_positions.len();
            active_indices = kept_positions
                .iter()
                .map(|&position| active_indices[position])
                .collect();

            let active_candidates: Vec<&QuotedPath> = active_indices.iter().map(|&idx| candidates[idx]).collect();

            current_split = if active_candidates.len() == 1 {
                let out = quote_fn(&active_candidates[0].path, total_amount)
                    .await
                    .map(|q| q.amount_out)
                    .unwrap_or(0);
                vec![(total_amount, out)]
            } else {
                self.optimize_n_paths(&active_candidates, total_amount, quote_fn).await
            };
        }

        (active_indices, current_split, dust_filtered_legs)
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
        // We use Brent's method to maximize f(x) = output_a(x * total) +
        // output_rest((1-x) * total)
        let optimal_fraction = self
            .brent_maximize(0.0, 1.0, |x| {
                let amount_a = (x * total_amount as f64) as u128;
                let amount_rest = total_amount.saturating_sub(amount_a);

                let path_a_clone = path_a.clone();
                let this = self;

                async move {
                    let out_a = quote_fn(&path_a_clone, amount_a)
                        .await
                        .map(|q| q.amount_out)
                        .unwrap_or(0);
                    let out_rest = Box::pin(this.total_out_for_paths(rest, amount_rest, quote_fn)).await;
                    (out_a + out_rest) as f64
                }
            })
            .await;

        let amount_a = (optimal_fraction * total_amount as f64) as u128;
        let amount_rest = total_amount.saturating_sub(amount_a);

        let out_a = quote_fn(path_a, amount_a).await.map(|q| q.amount_out).unwrap_or(0);

        let mut result = vec![(amount_a, out_a)];

        // Recurse on the rest
        if amount_rest > 0 && rest.len() > 0 {
            let rest_results = Box::pin(self.optimize_n_paths(rest, amount_rest, quote_fn)).await;
            result.extend(rest_results);
        }

        result
    }

    async fn total_out_for_paths<F, Fut>(&self, paths: &[&QuotedPath], total_amount: u128, quote_fn: &F) -> u128
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        if paths.is_empty() || total_amount == 0 {
            return 0;
        }

        if paths.len() == 1 {
            return quote_fn(&paths[0].path, total_amount)
                .await
                .map(|q| q.amount_out)
                .unwrap_or(0);
        }

        if paths.len() == 2 {
            return self
                .optimize_two_paths(&paths[0].path, &paths[1].path, total_amount, quote_fn)
                .await
                .iter()
                .map(|(_, out)| *out)
                .sum();
        }

        // N >= 3: O(N) weighted split by full-amount output (avoids nested Brent
        // explosion).
        self.approximate_weighted_paths_output(paths, total_amount, quote_fn)
            .await
    }

    /// Split `total_amount` across paths proportional to their full-amount
    /// quotes, then re-quote.
    async fn approximate_weighted_paths_output<F, Fut>(
        &self,
        paths: &[&QuotedPath],
        total_amount: u128,
        quote_fn: &F,
    ) -> u128
    where
        F: Fn(&Path, u128) -> Fut,
        Fut: std::future::Future<Output = Option<Quote>>,
    {
        if paths.is_empty() || total_amount == 0 {
            return 0;
        }
        let weights: Vec<u128> = paths.iter().map(|p| p.quote.amount_out.max(1)).collect();
        let weight_sum: u128 = weights.iter().copied().sum::<u128>().max(1);
        let mut allocated = 0u128;
        let mut total_out = 0u128;
        for (i, qp) in paths.iter().enumerate() {
            let amount = if i + 1 == paths.len() {
                total_amount.saturating_sub(allocated)
            } else {
                let share = (total_amount as u128).saturating_mul(weights[i]) / weight_sum;
                allocated += share;
                share
            };
            if amount == 0 {
                continue;
            }
            total_out += quote_fn(&qp.path, amount).await.map(|q| q.amount_out).unwrap_or(0);
        }
        total_out
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
                        quote_fn(&pa, amount_a).await.map(|q| q.amount_out).unwrap_or(0)
                    } else {
                        0
                    };
                    let out_b = if amount_b > 0 {
                        quote_fn(&pb, amount_b).await.map(|q| q.amount_out).unwrap_or(0)
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
            quote_fn(path_a, amount_a).await.map(|q| q.amount_out).unwrap_or(0)
        } else {
            0
        };
        let out_b = if amount_b > 0 {
            quote_fn(path_b, amount_b).await.map(|q| q.amount_out).unwrap_or(0)
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

/// Extra slippage on `minimum_out` for split routes (local quotes vs on-chain
/// multi-leg drift).
const SPLIT_MIN_OUTPUT_EXTRA_BPS: u32 = 150;

fn split_amount_in_fraction_bps(amount_in: u128, total_amount: u128) -> u32 {
    if total_amount == 0 {
        return 0;
    }
    (amount_in.saturating_mul(10_000) / total_amount) as u32
}

/// Leg implied rate must stay near the path's quote at the full trade size.
/// Catches bogus micro-amount quotes (e.g. empty Soroswap pools showing fantasy
/// output).
fn leg_rate_matches_full_quote(
    quoted_path: &QuotedPath,
    leg_amount_in: u128,
    leg_amount_out: u128,
    max_deviation_bps: u32,
) -> bool {
    if leg_amount_in == 0 || leg_amount_out == 0 {
        return false;
    }
    let full_in = quoted_path.quote.amount_in;
    let full_out = quoted_path.quote.amount_out;
    if full_in == 0 || full_out == 0 {
        return false;
    }
    let full_rate = full_out as f64 / full_in as f64;
    let leg_rate = leg_amount_out as f64 / leg_amount_in as f64;
    if full_rate <= 0.0 {
        return false;
    }
    let max_mul = 1.0 + max_deviation_bps as f64 / 10_000.0;
    let min_mul = 1.0 / max_mul;
    leg_rate <= full_rate * max_mul && leg_rate >= full_rate * min_mul
}

fn apply_slippage(amount: u128, slippage_bps: u32) -> u128 {
    amount * (10_000 - slippage_bps as u128) / 10_000
}

fn apply_split_minimum_slippage(amount: u128, slippage_bps: u32) -> u128 {
    apply_slippage(amount, slippage_bps.saturating_add(SPLIT_MIN_OUTPUT_EXTRA_BPS))
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
        debug: None,
    }
}

fn build_candidate_debug(candidates: &[&QuotedPath]) -> Vec<RouteDebugCandidate> {
    candidates
        .iter()
        .map(|candidate| RouteDebugCandidate {
            source: candidate.path.sources.join(" → "),
            path: candidate.path.tokens.iter().map(|token| token.canonical()).collect(),
            pool_addresses: candidate.path.pool_addresses.clone(),
            amount_out: candidate.quote.amount_out,
            price_impact_bps: candidate.quote.price_impact_bps,
        })
        .collect()
}

fn build_planned_split_debug(
    candidates: &[&QuotedPath],
    split_result: &[(u128, u128)],
    total_amount: u128,
) -> Vec<RouteDebugPlannedSplit> {
    split_result
        .iter()
        .enumerate()
        .filter(|(_, (amount, _))| *amount > 0)
        .map(|(i, (amount, out))| {
            let path = &candidates[i].path;
            RouteDebugPlannedSplit {
                source: path.sources.join(" → "),
                path: path.tokens.iter().map(|token| token.canonical()).collect(),
                pool_addresses: path.pool_addresses.clone(),
                amount_in: *amount,
                expected_amount_out: *out,
                fraction_bps: if total_amount == 0 {
                    0
                } else {
                    ((*amount * 10_000) / total_amount) as u32
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use {super::*, crate::types::TokenId};

    fn test_path(name: &str) -> Path {
        Path {
            tokens: vec![
                TokenId::Contract {
                    address: "token-in".to_string(),
                },
                TokenId::Contract {
                    address: "token-out".to_string(),
                },
            ],
            sources: vec![name.to_string()],
            pool_addresses: vec![format!("pool-{name}")],
            hops: 1,
        }
    }

    fn test_quote(path: &Path, amount_in: u128, amount_out: u128, price_impact_bps: u32) -> Quote {
        Quote {
            source: path.sources[0].clone(),
            pool_address: path.pool_addresses[0].clone(),
            token_in: path.tokens[0].clone(),
            token_out: path.tokens[1].clone(),
            amount_in,
            amount_out,
            price_impact_bps,
            fee_bps: 30,
            path: vec![],
            timestamp_ms: 0,
        }
    }

    /// Test Brent's method on a simple quadratic (maximum at x=0.6)
    #[tokio::test]
    async fn test_brent_quadratic() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());

        // f(x) = -(x - 0.6)^2 + 1, maximum at x = 0.6
        let result = optimizer
            .brent_maximize(0.0, 1.0, |x| async move { -(x - 0.6) * (x - 0.6) + 1.0 })
            .await;

        assert!((result - 0.6).abs() < 0.001, "Expected ~0.6, got {}", result);
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
        assert!(result > 0.5, "Expected x > 0.5 (favor deeper pool), got {}", result);
        assert!(result < 0.8, "Expected x < 0.8 (still use both), got {}", result);
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

    #[tokio::test]
    async fn test_no_split_when_only_one_path_exists() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());
        let path = test_path("solo");
        let quoted_paths = vec![QuotedPath {
            path: path.clone(),
            quote: test_quote(&path, 1_000, 990, 30),
        }];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |_path, amount| {
                let path = path.clone();
                async move { Some(test_quote(&path, amount, amount.saturating_sub(10), 30)) }
            })
            .await;

        assert!(!route.is_split);
        assert_eq!(route.sub_orders.len(), 1);
        assert_eq!(
            route.debug.as_ref().and_then(|d| d.split_rejected_reason.as_deref()),
            Some("not_enough_paths")
        );
    }

    #[tokio::test]
    async fn test_split_attempt_triggered_by_competitive_second_path() {
        let config = SplitConfig {
            split_threshold_bps: 100,
            split_competitive_delta_bps: 50,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 1_000, 800, 10),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 1_000, 798, 10),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |path, amount| {
                let path = path.clone();
                let path_name = path.sources[0].clone();
                async move {
                    let out = if path_name == "a" && amount <= 500 {
                        amount
                    } else if path_name == "b" && amount <= 500 {
                        amount.saturating_sub(1)
                    } else {
                        amount.saturating_sub(200)
                    };
                    Some(test_quote(&path, amount, out, 10))
                }
            })
            .await;

        assert!(
            route.debug.as_ref().is_some_and(|d| d.split_attempted),
            "expected competitive path trigger to attempt split"
        );
    }

    #[tokio::test]
    async fn test_split_skipped_when_competitive_but_zero_impact() {
        let config = SplitConfig {
            split_threshold_bps: 100,
            split_competitive_delta_bps: 50,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 1_000, 800, 0),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 1_000, 798, 0),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |_path, _amount| async {
                panic!("split optimizer should not re-quote when impact is zero");
            })
            .await;

        assert!(
            route.debug.as_ref().is_some_and(|d| !d.split_attempted),
            "zero impact should not trigger competitive split"
        );
    }

    #[tokio::test]
    async fn test_split_attempt_triggered_by_high_impact() {
        let config = SplitConfig {
            split_threshold_bps: 5,
            split_competitive_delta_bps: 0,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 1_000, 1_000, 25),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 1_000, 980, 8),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |path, amount| {
                let path = path.clone();
                async move { Some(test_quote(&path, amount, amount.saturating_sub(10), 25)) }
            })
            .await;

        assert!(
            route.debug.as_ref().is_some_and(|d| d.split_attempted),
            "expected high impact to attempt split"
        );
    }

    #[tokio::test]
    async fn test_fallback_to_single_when_split_has_no_improvement() {
        let config = SplitConfig {
            split_threshold_bps: 1,
            split_competitive_delta_bps: 1,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 1_000, 1_000, 20),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 1_000, 999, 19),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |path, amount| {
                let path = path.clone();
                async move {
                    let out = if path.sources[0] == "a" {
                        amount
                    } else {
                        amount.saturating_sub(1)
                    };
                    Some(test_quote(&path, amount, out, 20))
                }
            })
            .await;

        assert!(!route.is_split);
        assert_eq!(route.total_expected_out, 1_000);
        assert_eq!(
            route.debug.as_ref().and_then(|d| d.split_rejected_reason.as_deref()),
            Some("no_improvement")
        );
    }

    #[tokio::test]
    async fn test_three_path_split_can_beat_rest_best_approximation() {
        let config = SplitConfig {
            split_threshold_bps: 1,
            split_competitive_delta_bps: 10_000,
            min_split_fraction_bps: 0,
            max_splits: 3,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let path_c = test_path("c");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 1_000, 850, 30),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 1_000, 250, 10),
            },
            QuotedPath {
                path: path_c.clone(),
                quote: test_quote(&path_c, 1_000, 249, 10),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 1_000, 50, None, |path, amount| {
                let path = path.clone();
                async move {
                    let out = match path.sources[0].as_str() {
                        "a" => amount * 85 / 100,
                        "b" | "c" => amount.min(250),
                        _ => 0,
                    };
                    Some(test_quote(&path, amount, out, 10))
                }
            })
            .await;

        assert!(
            route.total_expected_out >= 920,
            "expected recursive optimizer to find a better 3-path split, got {}",
            route.total_expected_out
        );
    }

    #[test]
    fn leg_rate_rejects_fantasy_micro_quote() {
        let path = test_path("soroswap");
        let qp = QuotedPath {
            path: path.clone(),
            quote: test_quote(&path, 10_000_000_000, 1_500_000_000, 5),
        };
        assert!(!leg_rate_matches_full_quote(&qp, 203, 1_860_223, 500));
        assert!(leg_rate_matches_full_quote(&qp, 500_000_000, 75_000_000, 500));
    }

    #[test]
    fn split_amount_in_fraction_bps_computes_share() {
        assert_eq!(split_amount_in_fraction_bps(203, 10_000_000_000), 0);
        assert_eq!(split_amount_in_fraction_bps(1_167_504, 10_000_000_000), 1);
        assert_eq!(split_amount_in_fraction_bps(68_021_335, 10_000_000_000), 68);
    }

    #[tokio::test]
    async fn test_filters_split_legs_with_fantasy_rate_and_dust_input() {
        let optimizer = SplitOptimizer::new(SplitConfig::default());
        let path_good = test_path("good");
        let path_bad = test_path("bad");
        let quoted_paths = vec![
            QuotedPath {
                path: path_good.clone(),
                quote: test_quote(&path_good, 10_000_000_000, 1_500_000_000, 5),
            },
            QuotedPath {
                path: path_bad.clone(),
                quote: test_quote(&path_bad, 10_000_000_000, 1_500_000_000, 5),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 10_000_000_000, 50, None, |path, amount| {
                let path = path.clone();
                async move {
                    let out = if path.sources[0] == "bad" && amount < 1_000_000 {
                        amount.saturating_mul(9_000)
                    } else {
                        amount * 1_500_000_000 / 10_000_000_000
                    };
                    Some(test_quote(&path, amount, out, 5))
                }
            })
            .await;

        assert!(
            route.sub_orders.iter().all(|leg| leg.path.sources[0] != "bad"),
            "fantasy micro-quote path should be dropped"
        );
    }

    #[tokio::test]
    async fn test_filters_split_legs_below_min_fraction_bps() {
        let config = SplitConfig {
            split_threshold_bps: 1,
            split_competitive_delta_bps: 10_000,
            min_split_fraction_bps: 5,
            min_split_amount_in_bps: 0,
            max_leg_rate_deviation_bps: 10_000,
            max_splits: 3,
            ..SplitConfig::default()
        };
        let optimizer = SplitOptimizer::new(config);
        let path_a = test_path("a");
        let path_b = test_path("b");
        let path_c = test_path("c");
        let quoted_paths = vec![
            QuotedPath {
                path: path_a.clone(),
                quote: test_quote(&path_a, 10_000, 7_500, 10),
            },
            QuotedPath {
                path: path_b.clone(),
                quote: test_quote(&path_b, 10_000, 4_500, 10),
            },
            QuotedPath {
                path: path_c.clone(),
                quote: test_quote(&path_c, 10_000, 4, 10),
            },
        ];

        let route = optimizer
            .optimize(&quoted_paths, 10_000, 50, None, |path, amount| {
                let path = path.clone();
                async move {
                    let out = match path.sources[0].as_str() {
                        "a" => amount * 3 / 4,
                        "b" => amount * 4_500 / 10_000,
                        "c" => amount * 4 / 10_000,
                        _ => 0,
                    };
                    Some(test_quote(&path, amount, out, 10))
                }
            })
            .await;

        assert!(
            route.sub_orders.iter().all(|leg| leg.path.sources[0] != "c"),
            "expected dust path c to be removed"
        );
        assert!(
            route.debug.as_ref().map(|d| d.dust_filtered_legs).unwrap_or(0) >= 1,
            "expected debug metadata to report filtered dust leg(s)"
        );
    }
}

//! Quote engine: the main orchestrator that ties together path finding,
//! quoting, and split optimization.

use crate::{
    path_finder::{PathFinder, PathFinderConfig},
    split_optimizer::{QuotedPath, SplitConfig, SplitOptimizer},
    types::*,
};
use dex_adapters::DexAdapter;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

const CLASSIC_SOURCE: &str = "classic_dex";

fn apply_slippage(amount: u128, slippage_bps: u32) -> u128 {
    amount * (10_000 - slippage_bps as u128) / 10_000
}

/// The main quote engine that coordinates all routing logic.
pub struct QuoteEngine {
    path_finder: RwLock<PathFinder>,
    split_optimizer: SplitOptimizer,
    adapters: RwLock<Vec<Arc<dyn DexAdapter>>>,
    /// All cached pool edges (one entry per token pair per pool; same pool may appear many times).
    cached_pools: RwLock<Vec<TradingPair>>,
}

impl QuoteEngine {
    pub fn new(path_finder_config: PathFinderConfig, split_config: SplitConfig) -> Self {
        Self {
            path_finder: RwLock::new(PathFinder::new(path_finder_config)),
            split_optimizer: SplitOptimizer::new(split_config),
            adapters: RwLock::new(Vec::new()),
            cached_pools: RwLock::new(Vec::new()),
        }
    }

    fn find_pool_edge<'a>(
        pools: &'a [TradingPair],
        pool_address: &str,
        token_in: &TokenId,
        token_out: &TokenId,
    ) -> Option<&'a TradingPair> {
        let in_key = token_in.canonical();
        let out_key = token_out.canonical();
        pools.iter().find(|p| {
            p.pool_address == pool_address
                && ((p.token_a.canonical() == in_key && p.token_b.canonical() == out_key)
                    || (p.token_b.canonical() == in_key && p.token_a.canonical() == out_key))
        })
    }

    /// Register a new DEX adapter and update the token graph.
    pub async fn register_adapter(&self, adapter: Arc<dyn DexAdapter>) {
        let source = adapter.id().to_string();
        info!(source = %source, "Registering DEX adapter");

        // Fetch trading pairs from the adapter
        match adapter.get_trading_pairs().await {
            Ok(pairs) => {
                let trading_pairs: Vec<TradingPair> = pairs
                    .into_iter()
                    .map(|p| TradingPair {
                        token_a: p.token_a,
                        token_b: p.token_b,
                        source: source.clone(),
                        pool_address: p.pool_address,
                        fee_bps: p.fee_bps,
                        reserve_a: p.reserve_a,
                        reserve_b: p.reserve_b,
                    })
                    .collect();

                {
                    let mut pf = self.path_finder.write().await;
                    pf.update_from_source(&source, &trading_pairs);
                }
                {
                    let mut cache = self.cached_pools.write().await;
                    cache.retain(|p| p.source != source);
                    cache.extend(trading_pairs.iter().cloned());
                }

                info!(
                    source = %source,
                    pairs = trading_pairs.len(),
                    "Adapter registered successfully"
                );
            }
            Err(e) => {
                warn!(source = %source, error = %e, "Failed to fetch pairs from adapter");
            }
        }

        self.adapters.write().await.push(adapter);
    }

    /// Update the path finder directly from cached pairs (no RPC needed).
    /// Used for instant startup from disk cache.
    /// Also stores pairs in cached_pools for local quote computation.
    pub async fn update_pairs_from_cache(&self, source: &str, pairs: &[TradingPair]) {
        {
            let mut pf = self.path_finder.write().await;
            pf.update_from_source(source, pairs);
        }

        let mut cache = self.cached_pools.write().await;
        cache.retain(|p| p.source != source);
        cache.extend(pairs.iter().cloned());

        info!(
            source = source,
            pairs = pairs.len(),
            "Path finder updated from cache"
        );
    }

    /// Remove a DEX adapter.
    pub async fn unregister_adapter(&self, adapter_id: &str) {
        self.adapters.write().await.retain(|a| a.id() != adapter_id);

        let mut pf = self.path_finder.write().await;
        pf.clear_cache();
    }

    /// Get all unique token addresses known to the engine.
    pub async fn get_all_tokens(&self) -> Vec<String> {
        let pools = self.cached_pools.read().await;
        let mut tokens: std::collections::HashSet<String> = std::collections::HashSet::new();
        for pair in pools.iter() {
            tokens.insert(pair.token_a.canonical());
            tokens.insert(pair.token_b.canonical());
        }
        let mut result: Vec<String> = tokens.into_iter().collect();
        result.sort();
        result
    }

    /// Get the optimal route for a trade.
    pub async fn get_route(&self, request: &RouteRequest) -> OptimalRoute {
        let start = std::time::Instant::now();
        let slippage_bps = request.slippage_bps.unwrap_or(50);
        let (max_hops, max_paths) = {
            let pf = self.path_finder.read().await;
            (
                request.max_hops.unwrap_or(pf.default_max_hops()),
                pf.default_max_paths(),
            )
        };

        // 1. Discover paths (read lock — graph updates take write lock briefly)
        let paths = {
            let pf = self.path_finder.read().await;
            pf.find_paths_with_limits(&request.token_in, &request.token_out, max_hops, max_paths)
        };

        if paths.is_empty() {
            debug!(
                token_in = %request.token_in,
                token_out = %request.token_out,
                "No paths found"
            );
            return OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            };
        }

        debug!(paths = paths.len(), "Paths discovered");

        // 2. Get quotes for each path at full amount
        let adapters = self.adapters.read().await;
        let mut quoted_paths: Vec<QuotedPath> = Vec::new();

        for path in &paths {
            if let Some(quote) = self.quote_path(path, request.amount_in, &adapters).await {
                quoted_paths.push(QuotedPath {
                    path: path.clone(),
                    quote,
                });
            }
        }

        if quoted_paths.is_empty() {
            return OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            };
        }

        debug!(quoted = quoted_paths.len(), "Paths quoted");

        let (classic_quoted_paths, soroban_quoted_paths): (Vec<QuotedPath>, Vec<QuotedPath>) =
            quoted_paths
                .into_iter()
                .partition(|quoted| Self::is_classic_only_path(&quoted.path));

        let best_classic_route =
            classic_quoted_paths
                .iter()
                .max_by_key(|quoted| quoted.quote.amount_out)
                .map(|quoted| {
                    let minimum_out = apply_slippage(quoted.quote.amount_out, slippage_bps);
                    OptimalRoute {
                        sub_orders: vec![SubOrder {
                            path: quoted.path.clone(),
                            amount_in: request.amount_in,
                            expected_amount_out: quoted.quote.amount_out,
                            fraction: 1.0,
                        }],
                        total_amount_in: request.amount_in,
                        total_expected_out: quoted.quote.amount_out,
                        price_impact_bps: quoted.quote.price_impact_bps,
                        is_split: false,
                        improvement_bps: 0,
                        minimum_out,
                        compute_time_ms: start.elapsed().as_millis() as u64,
                        debug: None,
                    }
                });

        let best_soroban_route = if soroban_quoted_paths.is_empty() {
            None
        } else {
            let adapters_clone = adapters.clone();
            Some(
                self.split_optimizer
                    .optimize(
                        &soroban_quoted_paths,
                        request.amount_in,
                        slippage_bps,
                        request.max_splits,
                        |path, amount| {
                            let adapters_ref = adapters_clone.clone();
                            let path_clone = path.clone();
                            async move { self.quote_path(&path_clone, amount, &adapters_ref).await }
                        },
                    )
                    .await,
            )
        };

        match (best_classic_route, best_soroban_route) {
            (Some(classic), Some(soroban)) => {
                if classic.total_expected_out > soroban.total_expected_out {
                    classic
                } else {
                    soroban
                }
            }
            (Some(classic), None) => classic,
            (None, Some(soroban)) => soroban,
            (None, None) => OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            },
        }
    }

    fn is_classic_only_path(path: &Path) -> bool {
        !path.sources.is_empty() && path.sources.iter().all(|source| source == CLASSIC_SOURCE)
    }

    /// Quote a single path by simulating each hop sequentially.
    /// Falls back to local AMM computation from cached reserves when adapter is unavailable.
    async fn quote_path(
        &self,
        path: &Path,
        amount_in: u128,
        adapters: &[Arc<dyn DexAdapter>],
    ) -> Option<Quote> {
        let mut current_amount = amount_in;
        let mut total_fee_bps: u32 = 0;
        let mut max_impact_bps: u32 = 0;
        let cached_pools = self.cached_pools.read().await;

        for (i, source) in path.sources.iter().enumerate() {
            let token_in = &path.tokens[i];
            let token_out = &path.tokens[i + 1];
            let pool_address = &path.pool_addresses[i];

            let adapter = adapters.iter().find(|a| a.id() == source);

            let hop_result = if let Some(adapter) = adapter {
                adapter
                    .get_quote(token_in, token_out, current_amount, pool_address)
                    .await
                    .ok()
                    .flatten()
            } else {
                self.local_quote(
                    token_in,
                    token_out,
                    current_amount,
                    pool_address,
                    source,
                    &cached_pools,
                )
            };

            match hop_result {
                Some(hop_quote) => {
                    current_amount = hop_quote.amount_out;
                    total_fee_bps += hop_quote.fee_bps;
                    // Track the maximum per-hop impact (dominates the overall impact)
                    if hop_quote.price_impact_bps > max_impact_bps {
                        max_impact_bps = hop_quote.price_impact_bps;
                    }
                }
                None => return None,
            }
        }

        Some(Quote {
            source: path.sources.join("+"),
            pool_address: path.pool_addresses.join("+"),
            token_in: path.tokens.first()?.clone(),
            token_out: path.tokens.last()?.clone(),
            amount_in,
            amount_out: current_amount,
            price_impact_bps: max_impact_bps,
            fee_bps: total_fee_bps,
            path: path.tokens[1..path.tokens.len() - 1].to_vec(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Local quote computation using cached reserves and AMM formulas.
    fn local_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        cached_pools: &[TradingPair],
    ) -> Option<dex_adapters::AdapterQuote> {
        let pair = Self::find_pool_edge(cached_pools, pool_address, token_in, token_out)?;

        let (reserve_in, reserve_out) = if token_in.canonical() == pair.token_a.canonical() {
            (pair.reserve_a?, pair.reserve_b?)
        } else if token_in.canonical() == pair.token_b.canonical() {
            (pair.reserve_b?, pair.reserve_a?)
        } else {
            return None;
        };

        if reserve_in == 0 || reserve_out == 0 {
            return None;
        }

        // Apply appropriate AMM formula based on source
        let (amount_out, fee_bps) = match source {
            "soroswap" => {
                // Soroswap: fee = ceil(amount_in * 3 / 1000)
                let fee = (amount_in * 3 + 999) / 1000;
                let in_after_fee = amount_in - fee;
                let out = in_after_fee * reserve_out / (reserve_in + in_after_fee);
                (out, 30u32)
            }
            "aquarius" => {
                // Aquarius: in_after_fee = amount_in * (10000 - fee_bps) / 10000
                let fee_bps = pair.fee_bps;
                let in_after_fee = amount_in * (10_000 - fee_bps as u128) / 10_000;
                let out = in_after_fee * reserve_out / (reserve_in + in_after_fee);
                (out, fee_bps)
            }
            "phoenix" => {
                // Phoenix: fee on output
                let fee_bps = pair.fee_bps;
                let gross = amount_in * reserve_out / (reserve_in + amount_in);
                let commission = gross * fee_bps as u128 / 10_000;
                (gross - commission, fee_bps)
            }
            _ => {
                // Generic constant product
                let fee_bps = pair.fee_bps;
                let in_after_fee = amount_in * (10_000 - fee_bps as u128) / 10_000;
                let out = in_after_fee * reserve_out / (reserve_in + in_after_fee);
                (out, fee_bps)
            }
        };

        if amount_out == 0 {
            return None;
        }

        // Price impact = 1 - actual_out / ideal_out
        // ideal_out = amount_in * reserve_out / reserve_in (spot price, no slippage)
        let ideal_out = amount_in * reserve_out / reserve_in;
        let price_impact_bps = if ideal_out > 0 && amount_out < ideal_out {
            ((ideal_out - amount_out) * 10_000 / ideal_out) as u32
        } else {
            0
        };

        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps,
            price_impact_bps,
        })
    }

    /// Get the (in_idx, out_idx) for a pool and swap tokens.
    /// Returns Some((0, 1)) if token_in == token_a && token_out == token_b,
    /// Some((1, 0)) if token_in == token_b && token_out == token_a,
    /// None if pool is unknown or tokens don't match.
    pub async fn get_pool_indices(
        &self,
        pool_address: &str,
        token_in: &TokenId,
        token_out: &TokenId,
    ) -> Option<(u32, u32)> {
        let pools = self.cached_pools.read().await;
        let pair = Self::find_pool_edge(&pools, pool_address, token_in, token_out)?;
        let in_key = token_in.canonical();
        if in_key == pair.token_a.canonical() {
            Some((0, 1))
        } else {
            Some((1, 0))
        }
    }

    /// Refresh trading pairs from all adapters.
    pub async fn refresh_pairs(&self) {
        let adapters = self.adapters.read().await;
        for adapter in adapters.iter() {
            let source = adapter.id().to_string();
            match adapter.get_trading_pairs().await {
                Ok(pairs) => {
                    let trading_pairs: Vec<TradingPair> = pairs
                        .into_iter()
                        .map(|p| TradingPair {
                            token_a: p.token_a,
                            token_b: p.token_b,
                            source: source.clone(),
                            pool_address: p.pool_address,
                            fee_bps: p.fee_bps,
                            reserve_a: p.reserve_a,
                            reserve_b: p.reserve_b,
                        })
                        .collect();

                    let mut pf = self.path_finder.write().await;
                    pf.update_from_source(&source, &trading_pairs);
                }
                Err(e) => {
                    warn!(source = %source, error = %e, "Failed to refresh pairs");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(id: &str) -> TokenId {
        TokenId::Contract {
            address: id.to_string(),
        }
    }

    fn pair(source: &str, pool: &str, reserve_a: u128, reserve_b: u128) -> TradingPair {
        TradingPair {
            token_a: token("token-in"),
            token_b: token("token-out"),
            source: source.to_string(),
            pool_address: pool.to_string(),
            fee_bps: 0,
            reserve_a: Some(reserve_a),
            reserve_b: Some(reserve_b),
        }
    }

    #[tokio::test]
    async fn quote_prefers_best_classic_single_route_over_soroban_split() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache(
                CLASSIC_SOURCE,
                &[pair(CLASSIC_SOURCE, "classic-pool", 100_000, 100_000)],
            )
            .await;
        engine
            .update_pairs_from_cache("soroswap", &[pair("soroswap", "soro-pool", 10_000, 10_000)])
            .await;
        engine
            .update_pairs_from_cache("aquarius", &[pair("aquarius", "aqua-pool", 10_000, 10_000)])
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 1_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(5),
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert_eq!(route.sub_orders[0].path.sources, vec![CLASSIC_SOURCE.to_string()]);
        assert!(!route.is_split);
    }

    #[tokio::test]
    async fn quote_prefers_soroban_route_without_mixing_classic_legs() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache(
                CLASSIC_SOURCE,
                &[pair(CLASSIC_SOURCE, "classic-pool", 10_000, 8_000)],
            )
            .await;
        engine
            .update_pairs_from_cache("soroswap", &[pair("soroswap", "soro-pool", 10_000, 10_000)])
            .await;
        engine
            .update_pairs_from_cache("aquarius", &[pair("aquarius", "aqua-pool", 10_000, 10_000)])
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 5_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(5),
            })
            .await;

        assert!(
            route
                .sub_orders
                .iter()
                .all(|order| order.path.sources.iter().all(|source| source != CLASSIC_SOURCE))
        );
    }
}

// /// Rough price impact estimation.
// /// Real implementation would use reserve data from each pool.
// fn estimate_price_impact(amount_in: u128, amount_out: u128, fee_bps: u32) -> u32 {
//     if amount_in == 0 || amount_out == 0 {
//         return 0;
//     }
//     // Remove fee effect to isolate pure price impact
//     let amount_out_no_fee = amount_out * 10_000 / (10_000 - fee_bps as u128);
//     // If output equals input (1:1 price), impact is 0
//     // Impact grows as output decreases relative to "fair" price
//     // This is a placeholder; real implementation needs pool reserve data
//     0
// }

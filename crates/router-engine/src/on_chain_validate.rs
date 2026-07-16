//! Re-quote selected routes hop-by-hop using on-chain pool math.

use {
    crate::{quote_engine::QuoteEngine, types::OptimalRoute},
    dex_adapters::{on_chain_quote, rpc::SorobanRpc},
    tracing::{debug, warn},
};

const SPLIT_MIN_OUTPUT_EXTRA_BPS: u32 = 150;

fn apply_slippage(amount: u128, slippage_bps: u32) -> u128 {
    amount * (10_000 - slippage_bps as u128) / 10_000
}

fn apply_minimum_out(amount: u128, slippage_bps: u32, is_split: bool) -> u128 {
    if is_split {
        let extra = SPLIT_MIN_OUTPUT_EXTRA_BPS.min(10_000u32.saturating_sub(slippage_bps));
        apply_slippage(amount, slippage_bps.saturating_add(extra))
    } else {
        apply_slippage(amount, slippage_bps)
    }
}

/// Replace local hop amounts with on-chain `estimate_swap` / fresh Soroswap
/// reserves.
pub async fn apply_on_chain_hop_validation(
    rpc: &SorobanRpc,
    engine: &QuoteEngine,
    mut route: OptimalRoute,
    slippage_bps: u32,
) -> OptimalRoute {
    if route.sub_orders.is_empty() {
        return route;
    }

    let mut total_out = 0u128;
    let mut adjusted = false;
    let mut any_validated = false;

    for sub in &mut route.sub_orders {
        let local_out = sub.expected_amount_out;
        let mut in_indices = Vec::with_capacity(sub.path.hops);
        let mut out_indices = Vec::with_capacity(sub.path.hops);
        let mut indices_ok = true;

        for i in 0..sub.path.hops {
            let token_in = &sub.path.tokens[i];
            let token_out = &sub.path.tokens[i + 1];
            let pool = &sub.path.pool_addresses[i];
            match engine.get_pool_indices(pool, token_in, token_out).await {
                Some((in_idx, out_idx)) => {
                    in_indices.push(in_idx);
                    out_indices.push(out_idx);
                }
                None => {
                    warn!(
                        pool,
                        token_in = %token_in.canonical(),
                        token_out = %token_out.canonical(),
                        "on-chain validate: missing pool indices"
                    );
                    indices_ok = false;
                    break;
                }
            }
        }

        if !indices_ok {
            total_out = total_out.saturating_add(local_out);
            continue;
        }

        let tokens: Vec<String> = sub.path.tokens.iter().map(|t| t.canonical()).collect();
        match on_chain_quote::path_amount_out_on_chain(
            rpc,
            &sub.path.sources,
            &sub.path.pool_addresses,
            &tokens,
            &in_indices,
            &out_indices,
            sub.amount_in,
        )
        .await
        {
            Ok(Some(chain_out)) if chain_out > 0 => {
                any_validated = true;
                if chain_out != local_out {
                    debug!(
                        source = %sub.path.sources.join("+"),
                        local_out,
                        chain_out,
                        delta = local_out as i128 - chain_out as i128,
                        "on-chain hop validation adjusted sub-order output"
                    );
                    adjusted = true;
                }
                sub.expected_amount_out = chain_out;
                total_out = total_out.saturating_add(chain_out);
            }
            Ok(_) => {
                warn!(
                    source = %sub.path.sources.join("+"),
                    amount_in = sub.amount_in,
                    "on-chain hop validation failed; keeping local quote"
                );
                total_out = total_out.saturating_add(local_out);
            }
            Err(e) => {
                warn!(
                    source = %sub.path.sources.join("+"),
                    error = %e,
                    "on-chain hop validation error; keeping local quote"
                );
                total_out = total_out.saturating_add(local_out);
            }
        }
    }

    if any_validated && adjusted {
        route.total_expected_out = total_out;
        route.minimum_out = apply_minimum_out(total_out, slippage_bps, route.is_split);
        if let Some(debug) = route.debug.as_mut() {
            debug.split_total_out = Some(total_out);
        }
    }

    route
}

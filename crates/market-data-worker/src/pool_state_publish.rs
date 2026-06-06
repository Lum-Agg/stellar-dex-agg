//! Collect live pool state from adapters for Redis (topology snapshot excludes
//! reserves).

use {
    dex_adapters::{AquariusAdapter, DexAdapter},
    market_snapshot::pool_state_store::{AquariusPoolStateValue, XykPoolStateValue},
    std::{collections::HashSet, sync::Arc},
};

const XYK_REDIS_SOURCES: &[&str] = &["soroswap", "phoenix", "comet"];
/// Do not publish xy=k pools with dust on either side (router skips these too).
const MIN_XYK_RESERVE_STROOPS: u128 = 100_000_000;

/// xy=k reserves from adapter caches (not written into topology snapshot).
pub async fn collect_xyk_pool_state(adapters: &[Arc<dyn DexAdapter>]) -> Vec<XykPoolStateValue> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for adapter in adapters {
        if !XYK_REDIS_SOURCES.contains(&adapter.id()) {
            continue;
        }
        let source = adapter.id();
        for pair in adapter.get_cached_pairs().await {
            let (Some(reserve_a), Some(reserve_b)) = (pair.reserve_a, pair.reserve_b) else {
                continue;
            };
            if reserve_a == 0 ||
                reserve_b == 0 ||
                reserve_a < MIN_XYK_RESERVE_STROOPS ||
                reserve_b < MIN_XYK_RESERVE_STROOPS
            {
                continue;
            }
            let pool_key = XykPoolStateValue::pool_key(source, &pair.pool_address);
            if !seen.insert(pool_key) {
                continue;
            }
            out.push(XykPoolStateValue::new(
                source,
                &pair.pool_address,
                &pair.token_a.canonical(),
                &pair.token_b.canonical(),
                pair.fee_bps,
                reserve_a,
                reserve_b,
            ));
        }
    }

    out.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.pool_address.cmp(&b.pool_address))
    });
    out
}

/// Aquarius pools: token-ordered reserves + stable params (one key per pool
/// contract).
pub async fn collect_aquarius_pool_state(aquarius: &AquariusAdapter) -> Vec<AquariusPoolStateValue> {
    aquarius
        .export_pool_quote_states()
        .await
        .into_iter()
        .map(|state| AquariusPoolStateValue {
            pool_address: state.pool_address,
            tokens: state.tokens,
            reserves: state.reserves,
            fee_bps: state.fee_bps,
            is_stable: state.is_stable,
            amp: state.amp,
        })
        .collect()
}

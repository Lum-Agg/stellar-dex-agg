//! Collect live pool state from adapters for Redis (topology snapshot excludes
//! reserves).

use {
    dex_adapters::{comet::CometAdapter, AquariusAdapter, CometPoolQuoteState, DexAdapter},
    market_snapshot::pool_state_store::{
        AquariusPoolStateValue, CometPoolStateValue, CometTokenRecordValue, XykPoolStateValue,
    },
    std::{collections::HashSet, sync::Arc},
};

const XYK_REDIS_SOURCES: &[&str] = &["soroswap", "phoenix"];

pub fn comet_state_to_value(pool_address: &str, state: &CometPoolQuoteState) -> CometPoolStateValue {
    CometPoolStateValue {
        pool_address: pool_address.to_string(),
        records: state
            .records
            .iter()
            .map(|(token, record)| {
                (
                    token.clone(),
                    CometTokenRecordValue {
                        balance: record.balance,
                        weight: record.weight,
                        scalar: record.scalar,
                    },
                )
            })
            .collect(),
        swap_fee: state.swap_fee,
        updated_at_ms: 0,
    }
}

/// xy=k reserves from adapter caches (not written into topology snapshot).
/// Publishes pools with known reserves (including zero); quote_engine skips
/// unusable pools.
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

/// Comet weighted pool state (one Redis key per pool contract).
pub async fn collect_comet_pool_state(comet: &CometAdapter) -> Vec<CometPoolStateValue> {
    let mut out = Vec::new();
    for (pool_address, state) in comet.export_pool_quote_states().await {
        if state.records.len() < 2 {
            continue;
        }
        out.push(comet_state_to_value(&pool_address, &state));
    }
    out.sort_by(|a, b| a.pool_address.cmp(&b.pool_address));
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
            updated_at_ms: 0,
        })
        .collect()
}

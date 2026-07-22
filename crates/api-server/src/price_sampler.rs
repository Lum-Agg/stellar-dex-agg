//! Background sampling of quote-engine USDC marks.

use {
    crate::{
        handlers::collect_common_balance_token_ids,
        price_mark::{mark_token_usdc, USDC_SAC, XLM_SAC},
        price_store::PriceStore,
        state::AppState,
    },
    std::{
        collections::HashSet,
        sync::Arc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
    tracing::warn,
};

const EURC_SAC: &str = "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV";
const AQUA_SAC: &str = "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK";

/// Starts periodic price mark sampling. The caller decides whether sampling is
/// enabled.
pub fn spawn_price_sampler(state: AppState, store: Arc<PriceStore>) {
    let sample_secs = env_positive_u64("PRICE_SAMPLE_SECS").unwrap_or(600);
    let common_limit = env_usize("PRICE_SAMPLE_TOKEN_LIMIT").unwrap_or(30);
    let retention_days = env_positive_u64("PRICE_RETENTION_DAYS");
    let tokens = sample_tokens(collect_common_balance_token_ids(), common_limit);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(sample_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let now = unix_timestamp();

            for token in &tokens {
                match mark_token_usdc(&state, token).await {
                    Some((price_usdc, via)) => {
                        if let Err(error) = store.insert_tick(token, now, price_usdc, via) {
                            warn!(token, %error, "price sampler skipped tick insert");
                        }
                    }
                    None => warn!(token, "price sampler skipped unpriceable token"),
                }
            }

            if let Some(days) = retention_days {
                let retention_secs = days.saturating_mul(86_400).min(i64::MAX as u64) as i64;
                let cutoff = now.saturating_sub(retention_secs);
                if let Err(error) = store.prune_older_than(cutoff) {
                    warn!(%error, "price sampler retention prune failed");
                }
            }
        }
    });
}

fn sample_tokens(common_tokens: Vec<String>, common_limit: usize) -> Vec<String> {
    let priority = [XLM_SAC, USDC_SAC, EURC_SAC, AQUA_SAC];
    let mut seen: HashSet<String> = priority.iter().map(|token| (*token).to_string()).collect();
    let mut tokens: Vec<String> = priority.iter().map(|token| (*token).to_string()).collect();

    for token in common_tokens {
        if tokens.len().saturating_sub(priority.len()) == common_limit {
            break;
        }
        if seen.insert(token.clone()) {
            tokens.push(token);
        }
    }
    tokens
}

fn env_positive_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok().filter(|value| *value > 0)
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::sample_tokens;

    #[test]
    fn whitelist_keeps_priority_tokens_and_caps_common_tokens() {
        let tokens = sample_tokens(
            vec![
                "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string(),
                "common-1".to_string(),
                "common-2".to_string(),
            ],
            1,
        );

        assert_eq!(
            tokens,
            vec![
                "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
                "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
                "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
                "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK",
                "common-1",
            ]
        );
    }
}

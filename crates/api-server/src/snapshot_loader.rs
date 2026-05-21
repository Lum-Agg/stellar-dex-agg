use anyhow::Result;
use market_snapshot::{MarketSnapshot, TradingPairSnapshot};
use router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine};

use crate::{config::AppConfig, state::sanitize_cached_pairs};

fn snapshot_pair_to_trading(
    pair: &TradingPairSnapshot,
    source: &str,
) -> router_engine::TradingPair {
    router_engine::TradingPair {
        token_a: router_engine::TokenId::from_str_auto(&pair.token_a),
        token_b: router_engine::TokenId::from_str_auto(&pair.token_b),
        source: source.to_string(),
        pool_address: pair.pool_address.clone(),
        fee_bps: pair.fee_bps,
        reserve_a: pair.reserve_a,
        reserve_b: pair.reserve_b,
    }
}

pub async fn build_engine_from_snapshot(
    config: &AppConfig,
    snapshot: &MarketSnapshot,
) -> Result<QuoteEngine> {
    let split_config = SplitConfig {
        split_threshold_bps: config.split_threshold_bps,
        split_competitive_delta_bps: config.split_competitive_delta_bps,
        min_split_fraction_bps: config.min_split_fraction_bps,
        max_splits: config.max_splits,
        ..SplitConfig::default()
    };
    let engine = QuoteEngine::new(PathFinderConfig::default(), split_config);

    for source in &snapshot.sources {
        let trading_pairs = source
            .pairs
            .iter()
            .map(|pair| snapshot_pair_to_trading(pair, &source.source))
            .collect::<Vec<_>>();
        let trading_pairs = sanitize_cached_pairs(&source.source, trading_pairs);
        engine
            .update_pairs_from_cache(&source.source, &trading_pairs)
            .await;
    }

    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use market_snapshot::{MarketSnapshot, SourceSnapshot, TradingPairSnapshot};
    use router_engine::TokenId;

    fn sample_snapshot() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "v1",
            123,
            "mainnet",
            vec![SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "token-a".to_string(),
                    token_b: "token-b".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                    reserve_a: Some(1_000_000),
                    reserve_b: Some(2_000_000),
                }],
            }],
        )
    }

    #[test]
    fn loads_snapshot_from_current_file() {
        let dir = std::env::temp_dir().join(format!(
            "lumagg-snapshot-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(market_snapshot::CURRENT_SNAPSHOT_FILE),
            serde_json::to_vec(&sample_snapshot()).unwrap(),
        )
        .unwrap();

        let snapshot = market_snapshot::load_snapshot_from_dir(&dir).unwrap();
        assert_eq!(snapshot.version, "v1");
    }

    #[tokio::test]
    async fn builds_engine_from_snapshot_data() {
        let config = crate::config::AppConfig::default();
        let engine = build_engine_from_snapshot(&config, &sample_snapshot())
            .await
            .unwrap();

        let route = engine
            .get_route(&router_engine::RouteRequest {
                token_in: TokenId::Contract {
                    address: "token-a".to_string(),
                },
                token_out: TokenId::Contract {
                    address: "token-b".to_string(),
                },
                amount_in: 1_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert!(route.total_expected_out > 0);
    }
}

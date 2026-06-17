use {
    crate::{config::AppConfig, state::sanitize_cached_pairs},
    anyhow::Result,
    market_snapshot::{MarketSnapshot, TradingPairSnapshot},
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
};

pub fn path_finder_config_from_app(config: &AppConfig) -> PathFinderConfig {
    PathFinderConfig {
        max_hops: config.path_finder_max_hops,
        max_multi_hop_paths: config.path_finder_max_multi_hop_paths,
        max_direct_paths: config.path_finder_max_direct_paths,
        bridge_tokens: PathFinderConfig::default().bridge_tokens,
    }
}

fn snapshot_pair_to_trading(pair: &TradingPairSnapshot, source: &str) -> router_engine::TradingPair {
    router_engine::TradingPair {
        token_a: router_engine::TokenId::from_str_auto(&pair.token_a),
        token_b: router_engine::TokenId::from_str_auto(&pair.token_b),
        source: source.to_string(),
        pool_address: pair.pool_address.clone(),
        fee_bps: pair.fee_bps,
        reserve_a: None,
        reserve_b: None,
    }
}

pub async fn build_engine_from_snapshot(config: &AppConfig, snapshot: &MarketSnapshot) -> Result<QuoteEngine> {
    let split_config = SplitConfig {
        split_threshold_bps: config.split_threshold_bps,
        split_competitive_delta_bps: config.split_competitive_delta_bps,
        min_split_fraction_bps: config.min_split_fraction_bps,
        max_splits: config.max_splits,
        ..SplitConfig::default()
    };
    let engine = QuoteEngine::new(path_finder_config_from_app(config), split_config);

    for source in &snapshot.sources {
        let trading_pairs = source
            .pairs
            .iter()
            .map(|pair| snapshot_pair_to_trading(pair, &source.source))
            .collect::<Vec<_>>();
        let trading_pairs = sanitize_cached_pairs(&source.source, trading_pairs);
        engine.update_pairs_from_cache(&source.source, &trading_pairs).await;
    }

    Ok(engine)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        dex_adapters::clmm_math::{bitmap, sqrt_ratio_at_tick},
        market_snapshot::{ClmmPoolSnapshot, MarketSnapshot, SourceSnapshot, TradingPairSnapshot},
        router_engine::TokenId,
    };

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
                }],
            }],
        )
    }

    fn sample_clmm_pool_state() -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "pool-clmm".to_string(),
            token0: "token-a".to_string(),
            token1: "token-b".to_string(),
            fee_bps: 30,
            tick_spacing: 200,
            sqrt_price_x96: sqrt_ratio_at_tick(0).0,
            tick: 0,
            liquidity: 10_000_000_000_000,
            ticks: vec![
                market_snapshot::ClmmTickSnapshot {
                    tick: -1000,
                    liquidity_gross: 10_000_000_000_000,
                    liquidity_net: 10_000_000_000_000,
                },
                market_snapshot::ClmmTickSnapshot {
                    tick: 1000,
                    liquidity_gross: 10_000_000_000_000,
                    liquidity_net: -10_000_000_000_000,
                },
            ],
            chunk_bitmaps: vec![market_snapshot::ClmmBitmapWordSnapshot {
                word_pos: bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
                word: {
                    let mut word = [0u8; 32];
                    let lower_bit =
                        bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).1;
                    let upper_bit =
                        bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(1000, 200)).0).1;
                    word[31 - (lower_bit / 8) as usize] |= 1u8 << (lower_bit % 8);
                    word[31 - (upper_bit / 8) as usize] |= 1u8 << (upper_bit % 8);
                    word
                },
            }],
            word_bitmaps: vec![market_snapshot::ClmmBitmapWordSnapshot {
                word_pos: bitmap::word_bitmap_position(
                    bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
                )
                .0,
                word: {
                    let mut word = [0u8; 32];
                    let l2_bit = bitmap::word_bitmap_position(
                        bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
                    )
                    .1;
                    word[31 - (l2_bit / 8) as usize] |= 1u8 << (l2_bit % 8);
                    word
                },
            }],
            coverage: Some(market_snapshot::ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-1000),
                max_loaded_tick: Some(1000),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        }
    }

    fn sample_clmm_snapshot() -> MarketSnapshot {
        let pool = sample_clmm_pool_state();
        MarketSnapshot::from_sources(
            "v2",
            456,
            "mainnet",
            vec![SourceSnapshot {
                source: "sushi".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "token-a".to_string(),
                    token_b: "token-b".to_string(),
                    pool_address: "pool-clmm".to_string(),
                    fee_bps: 30,
                }],
            }],
        )
        .with_clmm_pool_refs(vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(&pool)])
    }

    async fn seed_clmm_quote_states(engine: &router_engine::QuoteEngine, pools: &[ClmmPoolSnapshot]) {
        use dex_adapters::clmm_math::clmm_pool_from_snapshot;
        for pool in pools {
            let (state, ticks) = clmm_pool_from_snapshot(pool);
            engine
                .update_clmm_quote_state(
                    &pool.source,
                    &pool.pool_address,
                    state,
                    ticks,
                    pool.coverage
                        .as_ref()
                        .map(|coverage| coverage.is_complete)
                        .unwrap_or(false),
                    pool.coverage.clone(),
                )
                .await;
        }
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
        let engine = build_engine_from_snapshot(&config, &sample_snapshot()).await.unwrap();
        engine
            .update_pairs_from_cache(
                "soroswap",
                &[router_engine::TradingPair {
                    token_a: TokenId::Contract {
                        address: "token-a".to_string(),
                    },
                    token_b: TokenId::Contract {
                        address: "token-b".to_string(),
                    },
                    source: "soroswap".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                    reserve_a: Some(1_000_000_000),
                    reserve_b: Some(2_000_000_000),
                }],
            )
            .await;

        let route = engine
            .get_route(&router_engine::RouteRequest {
                token_in: TokenId::Contract {
                    address: "token-a".to_string(),
                },
                token_out: TokenId::Contract {
                    address: "token-b".to_string(),
                },
                amount_in: 100_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert!(route.total_expected_out > 0);
    }

    #[tokio::test]
    async fn builds_engine_with_snapshot_clmm_quote_state() {
        let config = crate::config::AppConfig::default();
        let pool = sample_clmm_pool_state();
        let engine = build_engine_from_snapshot(&config, &sample_clmm_snapshot())
            .await
            .unwrap();
        seed_clmm_quote_states(&engine, std::slice::from_ref(&pool)).await;

        let route = engine
            .get_route(&router_engine::RouteRequest {
                token_in: TokenId::Contract {
                    address: "token-a".to_string(),
                },
                token_out: TokenId::Contract {
                    address: "token-b".to_string(),
                },
                amount_in: 1_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert!(route.total_expected_out > 0);
    }
}

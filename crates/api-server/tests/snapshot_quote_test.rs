use std::sync::Arc;

use anyhow::Result;
use api_server::{config::AppConfig, snapshot_loader::build_engine_from_snapshot};
use async_trait::async_trait;
use dex_adapters::clmm_math::clmm_pool_from_snapshot;
use dex_adapters::{
    clmm_math::{bitmap, sqrt_ratio_at_tick},
    AdapterQuote, AdapterTradingPair, DexAdapter, ProtocolType, SwapOperation, TokenId,
};
use market_snapshot::{
    ClmmBitmapWordSnapshot, ClmmCoverageSnapshot, ClmmPoolRefSnapshot, ClmmPoolSnapshot,
    ClmmTickSnapshot, MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
};
use router_engine::RouteRequest;

fn token(id: &str) -> TokenId {
    TokenId::Contract {
        address: id.to_string(),
    }
}

fn clmm_word_for_bits(bits: &[u32]) -> [u8; 32] {
    let mut word = [0u8; 32];
    for bit in bits {
        word[31 - (*bit / 8) as usize] |= 1u8 << (*bit % 8);
    }
    word
}

fn sample_clmm_pool_state(source: &str) -> ClmmPoolSnapshot {
    let lower_chunk = bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0;
    let upper_chunk = bitmap::chunk_address(bitmap::compress_tick(1000, 200)).0;
    let (chunk_word_pos, lower_bit) = bitmap::chunk_bitmap_position(lower_chunk);
    let (_, upper_bit) = bitmap::chunk_bitmap_position(upper_chunk);
    let (l2_word_pos, l2_bit) = bitmap::word_bitmap_position(chunk_word_pos);

    ClmmPoolSnapshot {
        source: source.to_string(),
        pool_address: "pool-clmm".to_string(),
        token0: "token-a".to_string(),
        token1: "token-b".to_string(),
        fee_bps: 30,
        tick_spacing: 200,
        sqrt_price_x96: sqrt_ratio_at_tick(0).0,
        tick: 0,
        liquidity: 10_000_000_000_000,
        ticks: vec![
            ClmmTickSnapshot {
                tick: -1000,
                liquidity_gross: 10_000_000_000_000,
                liquidity_net: 10_000_000_000_000,
            },
            ClmmTickSnapshot {
                tick: 1000,
                liquidity_gross: 10_000_000_000_000,
                liquidity_net: -10_000_000_000_000,
            },
        ],
        chunk_bitmaps: vec![ClmmBitmapWordSnapshot {
            word_pos: chunk_word_pos,
            word: clmm_word_for_bits(&[lower_bit, upper_bit]),
        }],
        word_bitmaps: vec![ClmmBitmapWordSnapshot {
            word_pos: l2_word_pos,
            word: clmm_word_for_bits(&[l2_bit]),
        }],
        coverage: Some(ClmmCoverageSnapshot {
            is_complete: true,
            min_loaded_tick: Some(-1000),
            max_loaded_tick: Some(1000),
            scanned_word_start: None,
            scanned_word_end: None,
        }),
    }
}

fn sample_clmm_topology_snapshot(source: &str) -> (MarketSnapshot, ClmmPoolSnapshot) {
    let pool = sample_clmm_pool_state(source);
    let snapshot = MarketSnapshot::from_sources(
        format!("{source}-snapshot"),
        123,
        "mainnet",
        vec![SourceSnapshot {
            source: source.to_string(),
            pairs: vec![TradingPairSnapshot {
                token_a: "token-a".to_string(),
                token_b: "token-b".to_string(),
                pool_address: "pool-clmm".to_string(),
                fee_bps: 30,
            }],
        }],
    )
    .with_clmm_pool_refs(vec![ClmmPoolRefSnapshot::from_pool(&pool)]);
    (snapshot, pool)
}

async fn seed_clmm_quote_states(engine: &router_engine::QuoteEngine, pools: &[ClmmPoolSnapshot]) {
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

fn sample_classic_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "classic-snapshot",
        123,
        "mainnet",
        vec![SourceSnapshot {
            source: "classic_dex".to_string(),
            pairs: vec![TradingPairSnapshot {
                token_a: "token-a".to_string(),
                token_b: "token-b".to_string(),
                pool_address: "classic-pool".to_string(),
                fee_bps: 0,
            }],
        }],
    )
}

struct StaticClassicAdapter;

#[async_trait]
impl DexAdapter for StaticClassicAdapter {
    fn id(&self) -> &str {
        "classic_dex"
    }

    fn name(&self) -> &str {
        "Static Classic"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::ClassicDex
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        Ok(vec![AdapterTradingPair {
            token_a: token("token-a"),
            token_b: token("token-b"),
            pool_address: "classic-pool".to_string(),
            fee_bps: 0,
            reserve_a: None,
            reserve_b: None,
        }])
    }

    async fn get_quote(
        &self,
        _token_in: &TokenId,
        _token_out: &TokenId,
        amount_in: u128,
        _pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        Ok(Some(AdapterQuote {
            amount_out: amount_in + 123,
            fee_bps: 0,
            price_impact_bps: 0,
        }))
    }

    async fn build_swap_op(
        &self,
        _token_in: &TokenId,
        _token_out: &TokenId,
        _amount_in: u128,
        _min_amount_out: u128,
        _pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::ClassicPathPayment {
            send_asset: "native".to_string(),
            dest_asset: "USDC".to_string(),
            send_amount: 1,
            dest_min: 1,
            path: vec![],
        })
    }

    async fn health_check(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn snapshot_quote_succeeds_for_sushi_clmm_pool() {
    let config = AppConfig::default();
    let (snapshot, pool) = sample_clmm_topology_snapshot("sushi");
    let engine = build_engine_from_snapshot(&config, &snapshot)
        .await
        .unwrap();
    seed_clmm_quote_states(&engine, std::slice::from_ref(&pool)).await;

    let route = engine
        .get_route(&RouteRequest {
            token_in: token("token-a"),
            token_out: token("token-b"),
            amount_in: 1_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
        })
        .await;

    assert_eq!(route.sub_orders.len(), 1);
    assert!(route.total_expected_out > 0);
}

#[tokio::test]
async fn snapshot_quote_succeeds_for_aquarius_clmm_pool() {
    let config = AppConfig::default();
    let (snapshot, pool) = sample_clmm_topology_snapshot("aquarius_clmm");
    let engine = build_engine_from_snapshot(&config, &snapshot)
        .await
        .unwrap();
    seed_clmm_quote_states(&engine, std::slice::from_ref(&pool)).await;

    let route = engine
        .get_route(&RouteRequest {
            token_in: token("token-a"),
            token_out: token("token-b"),
            amount_in: 1_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
        })
        .await;

    assert_eq!(route.sub_orders.len(), 1);
    assert!(route.total_expected_out > 0);
}

#[tokio::test]
async fn snapshot_quote_succeeds_for_classic_when_live_adapter_is_attached() {
    let config = AppConfig::default();
    let engine = build_engine_from_snapshot(&config, &sample_classic_snapshot())
        .await
        .unwrap();
    engine
        .register_adapter(Arc::new(StaticClassicAdapter))
        .await;

    let route = engine
        .get_route(&RouteRequest {
            token_in: token("token-a"),
            token_out: token("token-b"),
            amount_in: 1_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
        })
        .await;

    assert_eq!(route.sub_orders.len(), 1);
    assert_eq!(route.total_expected_out, 1_123);
}

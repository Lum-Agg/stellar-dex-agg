use {
    anyhow::Result,
    api_server::{config::AppConfig, snapshot_loader::build_engine_from_snapshot},
    async_trait::async_trait,
    dex_adapters::{
        clmm_math::{bitmap, clmm_pool_from_snapshot, sqrt_ratio_at_tick},
        AdapterQuote, AdapterTradingPair, DexAdapter, ProtocolType, SwapOperation, TokenId,
    },
    market_snapshot::{
        store::{build_snapshot_store, SnapshotStoreBackend},
        ClmmBitmapWordSnapshot, ClmmCoverageSnapshot, ClmmPoolRefSnapshot, ClmmPoolSnapshot, ClmmTickSnapshot,
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    router_engine::RouteRequest,
    std::{
        net::TcpStream,
        path::PathBuf,
        process::{Child, Command, Stdio},
        sync::Arc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
};

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

fn clmm_pool_snapshot(source: &str, token0: &str, token1: &str, pool_address: &str) -> ClmmPoolSnapshot {
    let lower_chunk = bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0;
    let upper_chunk = bitmap::chunk_address(bitmap::compress_tick(1000, 200)).0;
    let (chunk_word_pos, lower_bit) = bitmap::chunk_bitmap_position(lower_chunk);
    let (_, upper_bit) = bitmap::chunk_bitmap_position(upper_chunk);
    let (l2_word_pos, l2_bit) = bitmap::word_bitmap_position(chunk_word_pos);

    ClmmPoolSnapshot {
        source: source.to_string(),
        pool_address: pool_address.to_string(),
        token0: token0.to_string(),
        token1: token1.to_string(),
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

fn smoke_clmm_pools() -> Vec<ClmmPoolSnapshot> {
    vec![
        clmm_pool_snapshot("sushi", "token-a", "token-b", "pool-sushi"),
        clmm_pool_snapshot("aquarius_clmm", "token-c", "token-d", "pool-aqua"),
    ]
}

fn smoke_snapshot() -> MarketSnapshot {
    let clmm_pools = smoke_clmm_pools();
    let clmm_refs: Vec<ClmmPoolRefSnapshot> = clmm_pools.iter().map(ClmmPoolRefSnapshot::from_pool).collect();
    MarketSnapshot::from_sources(
        "redis-smoke",
        123,
        "mainnet",
        vec![
            SourceSnapshot {
                source: "sushi".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "token-a".to_string(),
                    token_b: "token-b".to_string(),
                    pool_address: "pool-sushi".to_string(),
                    fee_bps: 30,
                }],
            },
            SourceSnapshot {
                source: "aquarius_clmm".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "token-c".to_string(),
                    token_b: "token-d".to_string(),
                    pool_address: "pool-aqua".to_string(),
                    fee_bps: 30,
                }],
            },
            SourceSnapshot {
                source: "classic_dex".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "token-e".to_string(),
                    token_b: "token-f".to_string(),
                    pool_address: "classic-pool".to_string(),
                    fee_bps: 0,
                }],
            },
        ],
    )
    .with_clmm_pool_refs(clmm_refs)
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
            token_a: token("token-e"),
            token_b: token("token-f"),
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
            amount_out: amount_in + 77,
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

struct RedisServerGuard {
    child: Child,
    _dir: PathBuf,
}

impl RedisServerGuard {
    fn start(port: u16) -> Option<Self> {
        let dir = std::env::temp_dir().join(format!(
            "lumagg-redis-smoke-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let child = Command::new("redis-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--save")
            .arg("")
            .arg("--appendonly")
            .arg("no")
            .arg("--dir")
            .arg(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        for _ in 0..40 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Some(Self { child, _dir: dir });
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        None
    }
}

impl Drop for RedisServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn redis_snapshot_store_smoke_quotes_sushi_aquarius_clmm_and_classic() {
    let port = 6395;
    let Some(_redis) = RedisServerGuard::start(port) else {
        eprintln!("redis-server unavailable; skipping Redis snapshot smoke test");
        return;
    };
    let redis_url = format!("redis://127.0.0.1:{port}/");
    let store = build_snapshot_store(
        SnapshotStoreBackend::Redis,
        None,
        Some(&redis_url),
        Some("lumagg:test:snapshot:events"),
        Some(3),
    )
    .unwrap();

    let snapshot = smoke_snapshot();
    store.publish_snapshot(&snapshot).await.unwrap();
    let loaded = store.load_current_snapshot().await.unwrap();
    assert_eq!(loaded.version, "redis-smoke");
    assert_eq!(loaded.clmm_pool_refs.len(), 2);

    let config = AppConfig::default();
    let engine = build_engine_from_snapshot(&config, &loaded).await.unwrap();
    seed_clmm_quote_states(&engine, &smoke_clmm_pools()).await;
    engine.register_adapter(Arc::new(StaticClassicAdapter)).await;

    let sushi_route = engine
        .get_route(&RouteRequest {
            token_in: token("token-a"),
            token_out: token("token-b"),
            amount_in: 1_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_soroban: None,
        })
        .await;
    assert!(sushi_route.total_expected_out > 0);

    let aqua_route = engine
        .get_route(&RouteRequest {
            token_in: token("token-c"),
            token_out: token("token-d"),
            amount_in: 1_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_soroban: None,
        })
        .await;
    assert!(aqua_route.total_expected_out > 0);

    let classic_route = engine
        .get_route(&RouteRequest {
            token_in: token("token-e"),
            token_out: token("token-f"),
            amount_in: 1_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_soroban: None,
        })
        .await;
    assert_eq!(classic_route.total_expected_out, 1_077);
}

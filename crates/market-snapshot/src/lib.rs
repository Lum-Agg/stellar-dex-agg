use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod pool_state_store;
pub mod store;

pub const DEFAULT_SNAPSHOT_DIR: &str = "data/snapshots";
pub const CURRENT_SNAPSHOT_FILE: &str = "current.json";
pub const CURRENT_META_FILE: &str = "meta.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub version: String,
    pub generated_at_ms: u64,
    pub network: String,
    pub meta: SnapshotMeta,
    pub sources: Vec<SourceSnapshot>,
    #[serde(default)]
    pub token_metadata: Vec<TokenMetadataSnapshot>,
    /// CLMM pool topology only (no slot0 / ticks / liquidity). Live state is in Redis `lumagg:pool:clmm:*`.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        alias = "clmm_pools"
    )]
    pub clmm_pool_refs: Vec<ClmmPoolRefSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotMeta {
    pub source_count: usize,
    pub pair_count: usize,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentSnapshotMeta {
    pub version: String,
    pub generated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub source: String,
    pub pairs: Vec<TradingPairSnapshot>,
}

/// Routing-graph edge (topology only). Reserves live in Redis `lumagg:pool:xyk:*`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TradingPairSnapshot {
    pub token_a: String,
    pub token_b: String,
    pub pool_address: String,
    pub fee_bps: u32,
}

/// CLMM pool identity for routing / pool index (no tick data).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClmmPoolRefSnapshot {
    pub source: String,
    pub pool_address: String,
    pub token0: String,
    pub token1: String,
    pub fee_bps: u32,
    pub tick_spacing: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenMetadataSnapshot {
    pub contract: String,
    pub symbol: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

impl ClmmPoolRefSnapshot {
    pub fn from_pool(pool: &ClmmPoolSnapshot) -> Self {
        Self {
            source: pool.source.clone(),
            pool_address: pool.pool_address.clone(),
            token0: pool.token0.clone(),
            token1: pool.token1.clone(),
            fee_bps: pool.fee_bps,
            tick_spacing: pool.tick_spacing,
        }
    }
}

/// Full CLMM pool state (Redis `lumagg:pool:clmm:*` only — not stored in topology snapshot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClmmPoolSnapshot {
    pub source: String,
    pub pool_address: String,
    pub token0: String,
    pub token1: String,
    /// Fee normalized to the 10_000 denominator used by local CLMM math.
    pub fee_bps: u32,
    pub tick_spacing: i32,
    pub sqrt_price_x96: [u64; 4],
    pub tick: i32,
    pub liquidity: u128,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticks: Vec<ClmmTickSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_bitmaps: Vec<ClmmBitmapWordSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_bitmaps: Vec<ClmmBitmapWordSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<ClmmCoverageSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClmmTickSnapshot {
    pub tick: i32,
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClmmBitmapWordSnapshot {
    pub word_pos: i32,
    pub word: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClmmCoverageSnapshot {
    #[serde(default)]
    pub is_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_loaded_tick: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_loaded_tick: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_word_start: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_word_end: Option<i32>,
}

pub fn load_snapshot_from_dir(snapshot_dir: &Path) -> anyhow::Result<MarketSnapshot> {
    let bytes = std::fs::read(snapshot_dir.join(CURRENT_SNAPSHOT_FILE))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn write_snapshot_to_dir(snapshot_dir: &Path, snapshot: &MarketSnapshot) -> anyhow::Result<()> {
    std::fs::create_dir_all(snapshot_dir)?;

    let snapshot_path = snapshot_dir.join(CURRENT_SNAPSHOT_FILE);
    let snapshot_tmp_path = snapshot_dir.join(format!("{}.tmp", CURRENT_SNAPSHOT_FILE));
    let meta_path = snapshot_dir.join(CURRENT_META_FILE);
    let meta_tmp_path = snapshot_dir.join(format!("{}.tmp", CURRENT_META_FILE));

    std::fs::write(&snapshot_tmp_path, serde_json::to_vec_pretty(snapshot)?)?;
    std::fs::rename(&snapshot_tmp_path, &snapshot_path)?;

    std::fs::write(&meta_tmp_path, serde_json::to_vec_pretty(&snapshot.current_meta())?)?;
    std::fs::rename(&meta_tmp_path, &meta_path)?;

    Ok(())
}

impl MarketSnapshot {
    pub fn from_sources(
        version: impl Into<String>,
        generated_at_ms: u64,
        network: impl Into<String>,
        sources: Vec<SourceSnapshot>,
    ) -> Self {
        let pair_count = sources.iter().map(|source| source.pairs.len()).sum();
        let token_count = sources
            .iter()
            .flat_map(|source| source.pairs.iter())
            .flat_map(|pair| [pair.token_a.clone(), pair.token_b.clone()])
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        Self {
            version: version.into(),
            generated_at_ms,
            network: network.into(),
            meta: SnapshotMeta {
                source_count: sources.len(),
                pair_count,
                token_count,
            },
            sources,
            token_metadata: Vec::new(),
            clmm_pool_refs: Vec::new(),
        }
    }

    pub fn current_meta(&self) -> CurrentSnapshotMeta {
        CurrentSnapshotMeta {
            version: self.version.clone(),
            generated_at_ms: self.generated_at_ms,
        }
    }

    pub fn with_token_metadata(mut self, token_metadata: Vec<TokenMetadataSnapshot>) -> Self {
        self.token_metadata = token_metadata;
        self
    }

    pub fn with_clmm_pool_refs(mut self, clmm_pool_refs: Vec<ClmmPoolRefSnapshot>) -> Self {
        self.clmm_pool_refs = clmm_pool_refs;
        self
    }

    /// Build topology refs from in-memory CLMM state (worker publish path).
    pub fn clmm_pool_refs_from_states(pools: &[ClmmPoolSnapshot]) -> Vec<ClmmPoolRefSnapshot> {
        pools.iter().map(ClmmPoolRefSnapshot::from_pool).collect()
    }

    pub fn token_addresses(&self) -> std::collections::BTreeSet<String> {
        self.sources
            .iter()
            .flat_map(|source| source.pairs.iter())
            .flat_map(|pair| [pair.token_a.clone(), pair.token_b.clone()])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_clmm_pool() -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "pool-clmm".to_string(),
            token0: "A".to_string(),
            token1: "B".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [1, 2, 3, 4],
            tick: 120,
            liquidity: 1_234_567,
            ticks: vec![
                ClmmTickSnapshot {
                    tick: 60,
                    liquidity_gross: 10,
                    liquidity_net: 5,
                },
                ClmmTickSnapshot {
                    tick: 120,
                    liquidity_gross: 20,
                    liquidity_net: -5,
                },
            ],
            chunk_bitmaps: vec![ClmmBitmapWordSnapshot {
                word_pos: 1,
                word: [7u8; 32],
            }],
            word_bitmaps: vec![ClmmBitmapWordSnapshot {
                word_pos: 0,
                word: [3u8; 32],
            }],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(60),
                max_loaded_tick: Some(120),
                scanned_word_start: Some(-1),
                scanned_word_end: Some(2),
            }),
        }
    }

    #[test]
    fn market_snapshot_round_trips_via_json() {
        let snapshot = MarketSnapshot {
            version: "v1".to_string(),
            generated_at_ms: 123,
            network: "mainnet".to_string(),
            meta: SnapshotMeta {
                source_count: 1,
                pair_count: 1,
                token_count: 2,
            },
            sources: vec![SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "POOL".to_string(),
                    fee_bps: 30,
                }],
            }],
            token_metadata: vec![TokenMetadataSnapshot {
                contract: "A".to_string(),
                symbol: "TOKA".to_string(),
                name: "Token A".to_string(),
                logo: None,
            }],
            clmm_pool_refs: vec![ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("reserve_a"));
        let restored: MarketSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, "v1");
        assert_eq!(restored.sources[0].pairs[0].pool_address, "POOL");
        assert_eq!(restored.token_metadata[0].symbol, "TOKA");
        assert_eq!(
            restored.clmm_pool_refs,
            vec![ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())]
        );
    }

    #[test]
    fn market_snapshot_derives_meta_from_sources() {
        let snapshot = MarketSnapshot::from_sources(
            "v2",
            456,
            "mainnet",
            vec![
                SourceSnapshot {
                    source: "a".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: "XLM".to_string(),
                        token_b: "USDC".to_string(),
                        pool_address: "pool-1".to_string(),
                        fee_bps: 30,
                    }],
                },
                SourceSnapshot {
                    source: "b".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: "USDC".to_string(),
                        token_b: "AQUA".to_string(),
                        pool_address: "pool-2".to_string(),
                        fee_bps: 5,
                    }],
                },
            ],
        );

        assert_eq!(snapshot.meta.source_count, 2);
        assert_eq!(snapshot.meta.pair_count, 2);
        assert_eq!(snapshot.meta.token_count, 3);
        assert_eq!(snapshot.current_meta().version, "v2");
    }

    #[test]
    fn writes_and_reads_snapshot_files() {
        let dir = std::env::temp_dir().join(format!(
            "market-snapshot-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let snapshot = MarketSnapshot::from_sources(
            "v3",
            789,
            "mainnet",
            vec![SourceSnapshot {
                source: "phoenix".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool".to_string(),
                    fee_bps: 10,
                }],
            }],
        );

        write_snapshot_to_dir(&dir, &snapshot).unwrap();
        let restored = load_snapshot_from_dir(&dir).unwrap();
        let meta: CurrentSnapshotMeta = serde_json::from_slice(
            &std::fs::read(dir.join(CURRENT_META_FILE)).unwrap(),
        )
        .unwrap();

        assert_eq!(restored.version, "v3");
        assert_eq!(meta.version, "v3");
    }

    #[test]
    fn market_snapshot_can_include_token_metadata() {
        let snapshot = MarketSnapshot::from_sources(
            "v4",
            999,
            "mainnet",
            vec![SourceSnapshot {
                source: "classic_dex".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "native".to_string(),
                    token_b: "USDC:issuer".to_string(),
                    pool_address: "pool".to_string(),
                    fee_bps: 30,
                }],
            }],
        )
        .with_token_metadata(vec![TokenMetadataSnapshot {
            contract: "native".to_string(),
            symbol: "XLM".to_string(),
            name: "Stellar Lumens".to_string(),
            logo: Some("logo".to_string()),
        }]);

        assert_eq!(snapshot.token_metadata.len(), 1);
        assert_eq!(snapshot.token_metadata[0].name, "Stellar Lumens");
    }

    #[test]
    fn market_snapshot_defaults_missing_clmm_state_for_legacy_json() {
        let legacy_json = r#"{
            "version":"v1",
            "generated_at_ms":123,
            "network":"mainnet",
            "meta":{"source_count":1,"pair_count":1,"token_count":2},
            "sources":[
                {
                    "source":"soroswap",
                    "pairs":[
                        {
                            "token_a":"A",
                            "token_b":"B",
                            "pool_address":"POOL",
                            "fee_bps":30,
                            "reserve_a":100,
                            "reserve_b":200
                        }
                    ]
                }
            ],
            "token_metadata":[]
        }"#;

        let restored: MarketSnapshot = serde_json::from_str(legacy_json).unwrap();

        assert!(restored.clmm_pool_refs.is_empty());
        assert_eq!(restored.sources[0].pairs[0].pool_address, "POOL");
    }

    #[test]
    fn legacy_json_with_full_clmm_pools_deserializes_to_refs() {
        let legacy_json = r#"{
            "version":"v1",
            "generated_at_ms":123,
            "network":"mainnet",
            "meta":{"source_count":1,"pair_count":1,"token_count":2},
            "sources":[{"source":"sushi","pairs":[{"token_a":"A","token_b":"B","pool_address":"pool-clmm","fee_bps":30}]}],
            "clmm_pools":[{
                "source":"sushi",
                "pool_address":"pool-clmm",
                "token0":"A",
                "token1":"B",
                "fee_bps":30,
                "tick_spacing":60,
                "sqrt_price_x96":[1,2,3,4],
                "tick":120,
                "liquidity":999,
                "ticks":[]
            }],
            "token_metadata":[]
        }"#;

        let restored: MarketSnapshot = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(restored.clmm_pool_refs.len(), 1);
        assert_eq!(restored.clmm_pool_refs[0].pool_address, "pool-clmm");
        assert_eq!(restored.clmm_pool_refs[0].tick_spacing, 60);
    }

    #[test]
    fn market_snapshot_can_attach_clmm_pool_refs() {
        let snapshot = MarketSnapshot::from_sources(
            "v5",
            1_234,
            "mainnet",
            vec![SourceSnapshot {
                source: "sushi".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool-clmm".to_string(),
                    fee_bps: 30,
                }],
            }],
        )
        .with_clmm_pool_refs(vec![ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())]);

        assert_eq!(snapshot.clmm_pool_refs.len(), 1);
        assert_eq!(snapshot.clmm_pool_refs[0].source, "sushi");
    }
}

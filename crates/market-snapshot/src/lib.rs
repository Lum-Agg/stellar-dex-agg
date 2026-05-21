use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub token_metadata: Vec<TokenMetadataSnapshot>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TradingPairSnapshot {
    pub token_a: String,
    pub token_b: String,
    pub pool_address: String,
    pub fee_bps: u32,
    pub reserve_a: Option<u128>,
    pub reserve_b: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenMetadataSnapshot {
    pub contract: String,
    pub symbol: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
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
                    reserve_a: Some(100),
                    reserve_b: Some(200),
                }],
            }],
            token_metadata: vec![TokenMetadataSnapshot {
                contract: "A".to_string(),
                symbol: "TOKA".to_string(),
                name: "Token A".to_string(),
                logo: None,
            }],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: MarketSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, "v1");
        assert_eq!(restored.sources[0].pairs[0].pool_address, "POOL");
        assert_eq!(restored.token_metadata[0].symbol, "TOKA");
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
                        reserve_a: Some(10),
                        reserve_b: Some(20),
                    }],
                },
                SourceSnapshot {
                    source: "b".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: "USDC".to_string(),
                        token_b: "AQUA".to_string(),
                        pool_address: "pool-2".to_string(),
                        fee_bps: 5,
                        reserve_a: Some(30),
                        reserve_b: Some(40),
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
                    reserve_a: Some(5),
                    reserve_b: Some(6),
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
                    reserve_a: Some(1),
                    reserve_b: Some(2),
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
}

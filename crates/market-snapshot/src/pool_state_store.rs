//! Per-pool Redis cache (short TTL) for xy=k reserves and CLMM quote state.
//!
//! See `docs/pool-state-architecture.md`.

use std::collections::HashMap;

use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::ClmmPoolSnapshot;

/// Default Redis EX for pool keys. Should exceed worker refresh duration (often 15–30s on mainnet).
pub const DEFAULT_POOL_STATE_TTL_SECS: u64 = 30;
pub const DEFAULT_QUOTE_HYDRATE_MAX_POOLS: usize = 12;

const XYK_KEY_PREFIX: &str = "lumagg:pool:xyk";
const CLMM_KEY_PREFIX: &str = "lumagg:pool:clmm";

/// xy=k reserves stored per pool (canonical token orientation from worker snapshot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XykPoolStateValue {
    pub source: String,
    pub pool_address: String,
    pub token_a: String,
    pub token_b: String,
    pub fee_bps: u32,
    pub reserve_a: u128,
    pub reserve_b: u128,
}

impl XykPoolStateValue {
    pub fn redis_key(source: &str, pool_address: &str) -> String {
        format!("{XYK_KEY_PREFIX}:{source}:{pool_address}")
    }

    pub fn pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }

    pub fn new(
        source: impl Into<String>,
        pool_address: impl Into<String>,
        token_a: impl Into<String>,
        token_b: impl Into<String>,
        fee_bps: u32,
        reserve_a: u128,
        reserve_b: u128,
    ) -> Self {
        Self {
            source: source.into(),
            pool_address: pool_address.into(),
            token_a: token_a.into(),
            token_b: token_b.into(),
            fee_bps,
            reserve_a,
            reserve_b,
        }
    }
}

impl ClmmPoolSnapshot {
    pub fn redis_key(source: &str, pool_address: &str) -> String {
        format!("{CLMM_KEY_PREFIX}:{source}:{pool_address}")
    }

    pub fn pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }
}

/// Only complete CLMM coverage may be written to Redis (shared across API instances).
pub fn should_publish_clmm_to_redis(pool: &ClmmPoolSnapshot) -> bool {
    pool.coverage
        .as_ref()
        .map(|coverage| coverage.is_complete)
        .unwrap_or(false)
}

pub struct RedisPoolStateStore {
    client: redis::Client,
    ttl_secs: u64,
}

impl RedisPoolStateStore {
    pub fn new(redis_url: &str, ttl_secs: u64) -> Result<Self> {
        Ok(Self {
            client: redis::Client::open(redis_url)?,
            ttl_secs: ttl_secs.max(1),
        })
    }

    pub fn with_default_ttl(redis_url: &str) -> Result<Self> {
        Self::new(redis_url, DEFAULT_POOL_STATE_TTL_SECS)
    }

    pub fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// Whether the topology snapshot key exists in Redis.
    pub async fn snapshot_exists(&self) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let exists: bool = redis::cmd("EXISTS")
            .arg("lumagg:snapshot:current")
            .query_async(&mut conn)
            .await?;
        Ok(exists)
    }

    /// Worker hot path: publish xy=k reserves and complete CLMM state (not in topology snapshot).
    pub async fn publish_pool_state(
        &self,
        xyk_values: &[XykPoolStateValue],
        clmm_pools: &[ClmmPoolSnapshot],
    ) -> Result<()> {
        self.set_xyk_batch(xyk_values).await?;
        let complete_clmm: Vec<&ClmmPoolSnapshot> = clmm_pools
            .iter()
            .filter(|pool| should_publish_clmm_to_redis(pool))
            .collect();
        self.set_clmm_batch(
            &complete_clmm
                .iter()
                .map(|p| (*p).clone())
                .collect::<Vec<_>>(),
        )
        .await?;
        tracing::debug!(
            xyk_written = xyk_values.len(),
            clmm_written = complete_clmm.len(),
            ttl_secs = self.ttl_secs,
            "Published per-pool state to Redis"
        );
        Ok(())
    }

    pub async fn fetch_xyk(
        &self,
        refs: &[(String, String)],
    ) -> Result<HashMap<String, XykPoolStateValue>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = refs
            .iter()
            .map(|(source, pool)| XykPoolStateValue::redis_key(source, pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for ((source, pool), bytes) in refs.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: XykPoolStateValue = serde_json::from_slice(&bytes)?;
            out.insert(XykPoolStateValue::pool_key(source, pool), value);
        }
        Ok(out)
    }

    pub async fn fetch_clmm(
        &self,
        refs: &[(String, String)],
    ) -> Result<HashMap<String, ClmmPoolSnapshot>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = refs
            .iter()
            .map(|(source, pool)| ClmmPoolSnapshot::redis_key(source, pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for ((source, pool), bytes) in refs.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: ClmmPoolSnapshot = serde_json::from_slice(&bytes)?;
            out.insert(ClmmPoolSnapshot::pool_key(source, pool), value);
        }
        Ok(out)
    }

    pub async fn set_xyk_batch(&self, values: &[XykPoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for value in values {
            let key = XykPoolStateValue::redis_key(&value.source, &value.pool_address);
            let bytes = serde_json::to_vec(value)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    pub async fn set_clmm_batch(&self, pools: &[ClmmPoolSnapshot]) -> Result<()> {
        if pools.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for pool in pools {
            if !should_publish_clmm_to_redis(pool) {
                continue;
            }
            let key = ClmmPoolSnapshot::redis_key(&pool.source, &pool.pool_address);
            let bytes = serde_json::to_vec(pool)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }
}

pub fn parse_pool_state_ttl_secs_from_env() -> u64 {
    std::env::var("POOL_STATE_TTL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_POOL_STATE_TTL_SECS)
        .max(1)
}

pub fn parse_quote_hydrate_max_pools_from_env() -> usize {
    std::env::var("QUOTE_HYDRATE_MAX_POOLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_QUOTE_HYDRATE_MAX_POOLS)
        .max(1)
}

pub fn build_pool_state_store(redis_url: &str) -> Result<RedisPoolStateStore> {
    RedisPoolStateStore::new(redis_url, parse_pool_state_ttl_secs_from_env())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClmmCoverageSnapshot;

    #[test]
    fn clmm_writeback_requires_complete_coverage() {
        let complete = ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "p1".to_string(),
            token0: "A".to_string(),
            token1: "B".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 1,
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-60),
                max_loaded_tick: Some(60),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        };
        let incomplete = ClmmPoolSnapshot {
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: false,
                min_loaded_tick: Some(-60),
                max_loaded_tick: Some(60),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
            ..complete.clone()
        };

        assert!(should_publish_clmm_to_redis(&complete));
        assert!(!should_publish_clmm_to_redis(&incomplete));
        assert!(!should_publish_clmm_to_redis(&ClmmPoolSnapshot {
            coverage: None,
            ..complete
        }));
    }

    #[test]
    fn xyk_redis_keys_are_stable() {
        assert_eq!(
            XykPoolStateValue::redis_key("soroswap", "POOL"),
            "lumagg:pool:xyk:soroswap:POOL"
        );
        assert_eq!(
            ClmmPoolSnapshot::redis_key("sushi", "POOL"),
            "lumagg:pool:clmm:sushi:POOL"
        );
    }
}

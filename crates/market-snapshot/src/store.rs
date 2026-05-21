use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use redis::{AsyncCommands, Script};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
    load_snapshot_from_dir, write_snapshot_to_dir, MarketSnapshot,
};

pub const DEFAULT_REDIS_EVENTS_CHANNEL: &str = "lumagg:snapshot:events";
pub const DEFAULT_REDIS_SNAPSHOT_HISTORY: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotStoreBackend {
    File,
    Redis,
}

impl SnapshotStoreBackend {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "file" => Ok(Self::File),
            "redis" => Ok(Self::Redis),
            other => Err(anyhow!("unsupported snapshot backend: {}", other)),
        }
    }
}

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    async fn load_current_snapshot(&self) -> Result<MarketSnapshot>;
    async fn publish_snapshot(&self, snapshot: &MarketSnapshot) -> Result<()>;
}

pub fn build_snapshot_store(
    backend: SnapshotStoreBackend,
    snapshot_dir: Option<PathBuf>,
    redis_url: Option<&str>,
    redis_channel: Option<&str>,
    keep_latest_versions: Option<usize>,
) -> Result<Arc<dyn SnapshotStore>> {
    match backend {
        SnapshotStoreBackend::File => {
            let snapshot_dir = snapshot_dir
                .ok_or_else(|| anyhow!("snapshot_dir is required for file snapshot backend"))?;
            Ok(Arc::new(FileSnapshotStore::new(snapshot_dir)))
        }
        SnapshotStoreBackend::Redis => {
            let redis_url = redis_url
                .ok_or_else(|| anyhow!("snapshot_redis_url is required for redis snapshot backend"))?;
            Ok(Arc::new(RedisSnapshotStore::with_options(
                redis_url,
                redis_channel.unwrap_or(DEFAULT_REDIS_EVENTS_CHANNEL),
                keep_latest_versions.unwrap_or(DEFAULT_REDIS_SNAPSHOT_HISTORY),
            )))
        }
    }
}

pub fn should_reload_snapshot_version(
    current_version: Option<&str>,
    observed_version: &str,
) -> bool {
    current_version != Some(observed_version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotListenerEvent {
    ListenerHealthy,
    ListenerDegraded,
    SnapshotVersion(String),
}

pub fn subscribe_to_snapshot_events(
    redis_url: &str,
    channel: &str,
) -> Result<mpsc::UnboundedReceiver<SnapshotListenerEvent>> {
    let client = redis::Client::open(redis_url)?;
    let channel = channel.to_string();
    let (sender, receiver) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut listener_healthy = false;

        loop {
            match client.get_async_pubsub().await {
                Ok(pubsub) => {
                    let (mut sink, mut stream) = pubsub.split();
                    if let Err(error) = sink.subscribe(&channel).await {
                        warn!(
                            "Failed to subscribe to snapshot events channel {}: {}",
                            channel, error
                        );
                    } else {
                        if !listener_healthy {
                            if sender.send(SnapshotListenerEvent::ListenerHealthy).is_err() {
                                return;
                            }
                            listener_healthy = true;
                        }

                        while let Some(message) = stream.next().await {
                            match message.get_payload::<String>() {
                                Ok(version) => {
                                    if sender
                                        .send(SnapshotListenerEvent::SnapshotVersion(version))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                Err(error) => {
                                    warn!("Failed to decode snapshot version event: {}", error);
                                }
                            }
                        }
                        warn!("Snapshot pub/sub stream ended for channel {}", channel);
                    }
                }
                Err(error) => {
                    warn!("Failed to connect snapshot pub/sub listener: {}", error);
                }
            }

            if listener_healthy {
                if sender.send(SnapshotListenerEvent::ListenerDegraded).is_err() {
                    return;
                }
                listener_healthy = false;
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    Ok(receiver)
}

#[derive(Debug, Clone)]
pub struct FileSnapshotStore {
    snapshot_dir: PathBuf,
}

impl FileSnapshotStore {
    pub fn new(snapshot_dir: impl Into<PathBuf>) -> Self {
        Self {
            snapshot_dir: snapshot_dir.into(),
        }
    }
}

#[async_trait]
impl SnapshotStore for FileSnapshotStore {
    async fn load_current_snapshot(&self) -> Result<MarketSnapshot> {
        load_snapshot_from_dir(&self.snapshot_dir)
    }

    async fn publish_snapshot(&self, snapshot: &MarketSnapshot) -> Result<()> {
        write_snapshot_to_dir(&self.snapshot_dir, snapshot)
    }
}

#[derive(Clone)]
pub struct RedisSnapshotStore {
    client: redis::Client,
    key_prefix: Arc<str>,
    events_channel: Arc<str>,
    keep_latest_versions: usize,
}

impl RedisSnapshotStore {
    pub fn new(redis_url: &str) -> Self {
        Self::with_options(
            redis_url,
            DEFAULT_REDIS_EVENTS_CHANNEL,
            DEFAULT_REDIS_SNAPSHOT_HISTORY,
        )
    }

    pub fn with_history_limit(redis_url: &str, keep_latest_versions: usize) -> Self {
        Self::with_options(
            redis_url,
            DEFAULT_REDIS_EVENTS_CHANNEL,
            keep_latest_versions,
        )
    }

    pub fn with_options(
        redis_url: &str,
        events_channel: impl Into<Arc<str>>,
        keep_latest_versions: usize,
    ) -> Self {
        Self {
            client: redis::Client::open(redis_url).expect("invalid redis url"),
            key_prefix: Arc::from("lumagg:snapshot"),
            events_channel: events_channel.into(),
            keep_latest_versions,
        }
    }

    pub fn current_key(&self) -> String {
        format!("{}:current", self.key_prefix)
    }

    pub fn versioned_snapshot_key(&self, version: &str) -> String {
        format!("{}:data:{}", self.key_prefix, version)
    }

    pub fn versioned_meta_key(&self, version: &str) -> String {
        format!("{}:meta:{}", self.key_prefix, version)
    }

    pub fn events_channel(&self) -> String {
        self.events_channel.to_string()
    }

    pub fn versions_index_key(&self) -> String {
        format!("{}:versions", self.key_prefix)
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
struct RetentionPlan {
    retained_versions: Vec<String>,
    stale_versions: Vec<String>,
}

#[cfg(test)]
fn dedupe_versions_by_latest(recorded_versions: &[String]) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for version in recorded_versions.iter().rev() {
        if seen.insert(version.as_str()) {
            deduped.push(version.clone());
        }
    }

    deduped.reverse();
    deduped
}

#[cfg(test)]
fn build_retention_plan(
    recorded_versions: &[String],
    keep_latest: usize,
    current_version: &str,
) -> RetentionPlan {
    let mut candidate_versions = recorded_versions.to_vec();
    candidate_versions.push(current_version.to_string());

    let deduped_candidates = dedupe_versions_by_latest(&candidate_versions);
    let keep_from = deduped_candidates
        .len()
        .saturating_sub(keep_latest.max(1));
    let retained_versions = deduped_candidates[keep_from..].to_vec();
    let retained_set: std::collections::HashSet<&str> =
        retained_versions.iter().map(String::as_str).collect();
    let stale_versions = dedupe_versions_by_latest(recorded_versions)
        .iter()
        .filter(|version| !retained_set.contains(version.as_str()))
        .cloned()
        .collect()
;

    RetentionPlan {
        retained_versions,
        stale_versions,
    }
}

fn publish_snapshot_script() -> Script {
    Script::new(
        r#"
local current_version = ARGV[1]
local snapshot_bytes = ARGV[2]
local meta_bytes = ARGV[3]
local keep_latest = tonumber(ARGV[4])

if not keep_latest or keep_latest < 1 then
    keep_latest = 1
end

local versions = redis.call('LRANGE', KEYS[4], 0, -1)
local previous_versions = {}
for i = 1, #versions do
    previous_versions[i] = versions[i]
end

versions[#versions + 1] = current_version

local seen = {}
local deduped_reversed = {}
for i = #versions, 1, -1 do
    local version = versions[i]
    if not seen[version] then
        seen[version] = true
        deduped_reversed[#deduped_reversed + 1] = version
    end
end

local retained_versions = {}
for i = #deduped_reversed, 1, -1 do
    retained_versions[#retained_versions + 1] = deduped_reversed[i]
end

while #retained_versions > keep_latest do
    table.remove(retained_versions, 1)
end

local retained_set = {}
for i = 1, #retained_versions do
    retained_set[retained_versions[i]] = true
end

local stale_versions = {}
local previous_seen = {}
for i = #previous_versions, 1, -1 do
    local version = previous_versions[i]
    if not previous_seen[version] then
        previous_seen[version] = true
        if not retained_set[version] then
            stale_versions[#stale_versions + 1] = version
        end
    end
end

redis.call('SET', KEYS[1], snapshot_bytes)
redis.call('SET', KEYS[2], meta_bytes)
redis.call('SET', KEYS[3], current_version)
redis.call('DEL', KEYS[4])
if #retained_versions > 0 then
    redis.call('RPUSH', KEYS[4], unpack(retained_versions))
end
redis.call('PUBLISH', ARGV[5], current_version)
for i = 1, #stale_versions do
    redis.call('DEL', ARGV[6] .. stale_versions[i])
    redis.call('DEL', ARGV[7] .. stale_versions[i])
end

return #stale_versions
"#,
    )
}

#[async_trait]
impl SnapshotStore for RedisSnapshotStore {
    async fn load_current_snapshot(&self) -> Result<MarketSnapshot> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let version: Option<String> = conn.get(self.current_key()).await?;
        let version = version.ok_or_else(|| anyhow!("no current snapshot version in redis"))?;
        let data: Vec<u8> = conn.get(self.versioned_snapshot_key(&version)).await?;
        Ok(serde_json::from_slice(&data)?)
    }

    async fn publish_snapshot(&self, snapshot: &MarketSnapshot) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let snapshot_bytes = serde_json::to_vec(snapshot)?;
        let meta_bytes = serde_json::to_vec(&snapshot.current_meta())?;
        let version = snapshot.version.clone();
        let data_key = self.versioned_snapshot_key(&version);
        let meta_key = self.versioned_meta_key(&version);
        let current_key = self.current_key();
        let versions_index_key = self.versions_index_key();
        let events_channel = self.events_channel();
        let data_prefix = format!("{}:data:", self.key_prefix);
        let meta_prefix = format!("{}:meta:", self.key_prefix);

        let _: i32 = publish_snapshot_script()
            .key(&data_key)
            .key(&meta_key)
            .key(&current_key)
            .key(&versions_index_key)
            .arg(&version)
            .arg(snapshot_bytes)
            .arg(meta_bytes)
            .arg(self.keep_latest_versions.max(1))
            .arg(&events_channel)
            .arg(data_prefix)
            .arg(meta_prefix)
            .invoke_async(&mut conn)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarketSnapshot, SourceSnapshot, TradingPairSnapshot};

    fn sample_snapshot(version: &str) -> MarketSnapshot {
        MarketSnapshot::from_sources(
            version,
            123,
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
    }

    #[tokio::test]
    async fn file_snapshot_store_publishes_and_loads_current_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "snapshot-store-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = FileSnapshotStore::new(dir);
        let snapshot = sample_snapshot("v-file");

        store.publish_snapshot(&snapshot).await.unwrap();
        let restored = store.load_current_snapshot().await.unwrap();

        assert_eq!(restored.version, "v-file");
    }

    #[test]
    fn redis_snapshot_store_uses_versioned_keys() {
        let store = RedisSnapshotStore::new("redis://127.0.0.1/");
        assert_eq!(store.current_key(), "lumagg:snapshot:current");
        assert_eq!(
            store.versioned_snapshot_key("v1"),
            "lumagg:snapshot:data:v1"
        );
        assert_eq!(
            store.versioned_meta_key("v1"),
            "lumagg:snapshot:meta:v1"
        );
        assert_eq!(store.events_channel(), "lumagg:snapshot:events");
        assert_eq!(store.versions_index_key(), "lumagg:snapshot:versions");
    }

    #[test]
    fn snapshot_reload_version_dedupe_only_reloads_new_versions() {
        assert!(should_reload_snapshot_version(None, "v1"));
        assert!(!should_reload_snapshot_version(Some("v1"), "v1"));
        assert!(should_reload_snapshot_version(Some("v1"), "v2"));
        assert!(!should_reload_snapshot_version(Some("v2"), "v2"));
    }

    #[test]
    fn redis_snapshot_store_uses_custom_event_channel() {
        let store = RedisSnapshotStore::with_options("redis://127.0.0.1/", "custom:snapshots", 7);

        assert_eq!(store.events_channel(), "custom:snapshots");
    }

    #[test]
    fn retention_plan_keeps_latest_n_versions() {
        let versions = vec![
            "v1".to_string(),
            "v2".to_string(),
            "v3".to_string(),
            "v4".to_string(),
            "v5".to_string(),
        ];
        let plan = build_retention_plan(&versions, 3, "v5");

        assert_eq!(
            plan.retained_versions,
            vec!["v3".to_string(), "v4".to_string(), "v5".to_string()]
        );
        assert_eq!(
            plan.stale_versions,
            vec!["v1".to_string(), "v2".to_string()]
        );
    }

    #[test]
    fn retention_plan_never_deletes_current_version() {
        let versions = vec![
            "v1".to_string(),
            "v2".to_string(),
            "v3".to_string(),
        ];
        let plan = build_retention_plan(&versions, 0, "v3");

        assert_eq!(plan.retained_versions, vec!["v3".to_string()]);
        assert_eq!(
            plan.stale_versions,
            vec!["v1".to_string(), "v2".to_string()]
        );
    }

    #[test]
    fn retention_plan_dedupes_versions_and_keeps_latest_position() {
        let versions = vec![
            "v1".to_string(),
            "v2".to_string(),
            "v1".to_string(),
            "v3".to_string(),
            "v2".to_string(),
        ];
        let plan = build_retention_plan(&versions, 3, "v2");

        assert_eq!(
            plan.retained_versions,
            vec!["v1".to_string(), "v3".to_string(), "v2".to_string()]
        );
        assert!(plan.stale_versions.is_empty());
    }

    #[test]
    fn retention_plan_deletes_only_versions_outside_final_retained_set() {
        let versions = vec![
            "v1".to_string(),
            "v2".to_string(),
            "v1".to_string(),
            "v3".to_string(),
            "v2".to_string(),
            "v4".to_string(),
        ];
        let plan = build_retention_plan(&versions, 2, "v4");

        assert_eq!(
            plan.retained_versions,
            vec!["v2".to_string(), "v4".to_string()]
        );
        assert_eq!(
            plan.stale_versions,
            vec!["v1".to_string(), "v3".to_string()]
        );
    }

    #[test]
    fn snapshot_store_backend_parses_known_values() {
        assert_eq!(
            SnapshotStoreBackend::parse("file").unwrap(),
            SnapshotStoreBackend::File
        );
        assert_eq!(
            SnapshotStoreBackend::parse("redis").unwrap(),
            SnapshotStoreBackend::Redis
        );
        assert!(SnapshotStoreBackend::parse("unknown").is_err());
    }
}

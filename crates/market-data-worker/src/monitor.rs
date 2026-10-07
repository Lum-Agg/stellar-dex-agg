//! Telegram heartbeat + failure alerts for the market-data worker.

use {
    crate::{clmm_metrics::ClmmCoverageMetrics, worker::WorkerShared},
    lumagg_alerts::TelegramAlerter,
    market_snapshot::pool_state_store::PoolStateStore,
    std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    tokio::sync::RwLock,
    tracing::warn,
};

pub struct WorkerMonitorMetrics {
    pub last_publish_ms: AtomicU64,
    pub last_xyk_count: AtomicU64,
    pub last_clmm_complete: AtomicU64,
}

impl WorkerMonitorMetrics {
    pub fn new() -> Self {
        Self {
            last_publish_ms: AtomicU64::new(0),
            last_xyk_count: AtomicU64::new(0),
            last_clmm_complete: AtomicU64::new(0),
        }
    }

    pub fn record_publish(&self, xyk: usize, clmm_complete: usize) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.last_publish_ms.store(now, Ordering::Relaxed);
        self.last_xyk_count.store(xyk as u64, Ordering::Relaxed);
        self.last_clmm_complete.store(clmm_complete as u64, Ordering::Relaxed);
    }
}

impl Default for WorkerMonitorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_telegram_monitor(
    alerter: Arc<TelegramAlerter>,
    metrics: Arc<WorkerMonitorMetrics>,
    clmm_metrics: Arc<ClmmCoverageMetrics>,
    shared: Arc<RwLock<WorkerShared>>,
    pool_state_store: Option<Arc<dyn PoolStateStore>>,
    api_health_url: String,
    rpc_url: String,
) {
    tokio::spawn(async move {
        let heartbeat_secs = std::env::var("TELEGRAM_HEARTBEAT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(heartbeat_secs.max(60)));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            interval.tick().await;
            let msg = match build_heartbeat_message(
                &metrics,
                &clmm_metrics,
                &shared,
                pool_state_store.as_deref(),
                &api_health_url,
                &rpc_url,
            )
            .await
            {
                Ok(m) => m,
                Err(error) => {
                    warn!("heartbeat message build failed: {}", error);
                    continue;
                }
            };
            if let Err(error) = alerter.send(&msg).await {
                warn!("telegram heartbeat failed: {}", error);
            }
        }
    });
}

pub async fn alert_failure(alerter: Option<&Arc<TelegramAlerter>>, key: &str, detail: &str) {
    let Some(alerter) = alerter else {
        return;
    };
    let text = format!("⚠️ LumAgg worker\n{detail}");
    if let Err(error) = alerter.alert(key, &text).await {
        warn!("telegram alert failed: {}", error);
    }
}

async fn build_heartbeat_message(
    metrics: &WorkerMonitorMetrics,
    clmm_metrics: &ClmmCoverageMetrics,
    shared: &RwLock<WorkerShared>,
    pool_store: Option<&dyn PoolStateStore>,
    api_health_url: &str,
    rpc_url: &str,
) -> anyhow::Result<String> {
    let guard = shared.read().await;
    let sources = guard.sources.len();
    let clmm_tracked = guard.clmm_pools.len();
    drop(guard);

    let last_pub = metrics.last_publish_ms.load(Ordering::Relaxed);
    let xyk = metrics.last_xyk_count.load(Ordering::Relaxed);
    let clmm_ok = metrics.last_clmm_complete.load(Ordering::Relaxed);
    let clmm_snap = clmm_metrics.snapshot();
    let clmm_skip_bps = ClmmCoverageMetrics::skip_rate_bps(clmm_snap);

    let api_ok = reqwest::Client::new()
        .get(api_health_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let snapshot_ok = pool_store.is_some();

    let stale = last_pub > 0
        && std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(last_pub)
            > 120;

    let rpc_line = format_rpc_block_height(rpc_url).await;

    Ok(format!(
        "✅ LumAgg heartbeat\n\
         API health ({api_health_url}): {}\n\
         Redis snapshot: {}\n\
         {rpc_line}\
         Pool publish stale (>120s): {}\n\
         Topology sources: {sources}\n\
         CLMM tracked: {clmm_tracked}\n\
         Last publish: xy:k={xyk} clmm_complete={clmm_ok}\n\
         CLMM refresh attempts: {clmm_attempts}\n\
         CLMM skipped incomplete: {clmm_skipped} ({clmm_skip_bps} bps)\n\
         CLMM published complete: {clmm_published}\n\
         last_publish_unix={last_pub}",
        if api_ok { "OK" } else { "FAIL" },
        if snapshot_ok { "OK" } else { "MISSING" },
        stale,
        clmm_attempts = clmm_snap.refresh_attempts,
        clmm_skipped = clmm_snap.publish_skipped_incomplete,
        clmm_published = clmm_snap.published_complete,
        clmm_skip_bps = clmm_skip_bps,
    ))
}

async fn format_rpc_block_height(rpc_url: &str) -> String {
    let mainnet_ref = std::env::var("MAINNET_RPC_REF_URL")
        .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());

    let local = fetch_latest_ledger(rpc_url).await;
    let mainnet = if rpc_url == mainnet_ref {
        None
    } else {
        fetch_latest_ledger(&mainnet_ref).await
    };

    match (local, mainnet) {
        (Some((local_h, local_proto)), Some((main_h, _))) => {
            let gap = main_h.saturating_sub(local_h);
            let sync = if gap <= 100 { "OK" } else { "LAGGING" };
            format!(
                "RPC block_height (local): {local_h} proto={local_proto}\n\
                 RPC block_height (mainnet): {main_h}\n\
                 RPC ledger gap: {gap} ({sync})\n"
            )
        }
        (Some((local_h, local_proto)), None) => {
            format!("RPC block_height: {local_h} proto={local_proto}\n")
        }
        (None, _) => "RPC block_height: unavailable\n".to_string(),
    }
}

async fn fetch_latest_ledger(rpc_url: &str) -> Option<(u32, u32)> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestLedger",
    });

    let resp = reqwest::Client::new()
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .ok()?;
    let parsed: serde_json::Value = resp.json().await.ok()?;
    let result = parsed.get("result")?;
    let sequence = result.get("sequence")?.as_u64()? as u32;
    let protocol_version = result.get("protocolVersion")?.as_u64()? as u32;
    Some((sequence, protocol_version))
}

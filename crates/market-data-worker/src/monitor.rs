//! Telegram heartbeat + failure alerts for the market-data worker.

use {
    crate::{clmm_metrics::ClmmCoverageMetrics, worker::WorkerShared},
    lumagg_alerts::TelegramAlerter,
    market_snapshot::pool_state_store::RedisPoolStateStore,
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
    pool_state_store: Option<Arc<RedisPoolStateStore>>,
    api_health_url: String,
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
    pool_store: Option<&RedisPoolStateStore>,
    api_health_url: &str,
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

    let snapshot_ok = if let Some(store) = pool_store {
        store.snapshot_exists().await.unwrap_or(false)
    } else {
        false
    };

    let stale = last_pub > 0 &&
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(last_pub) >
            120;

    Ok(format!(
        "✅ LumAgg heartbeat\n\
         API health ({api_health_url}): {}\n\
         Redis snapshot: {}\n\
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

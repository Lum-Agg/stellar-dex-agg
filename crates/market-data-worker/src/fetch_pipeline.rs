//! Ledger-driven pool-state fetch pipeline: high-priority `FetchTask` queue →
//! RPC workers → Redis sink.
//!
//! Full-market refresh is **not** scheduled here. Bootstrap + periodic
//! discovery publish pool state to Redis; this pipeline only handles
//! ledger-touched pools (Jupiter-style event-driven updates).

use {
    crate::worker::WorkerShared,
    anyhow::Result,
    dex_adapters::{
        aquarius::AquariusAdapter, aquarius_clmm::AquariusClmmAdapter,
        batch_refresh::batch_refresh_soroswap_reserves_parallel, comet::CometAdapter, phoenix::PhoenixAdapter,
        pool_index::PoolRef, rpc::SorobanRpc, soroswap::SoroswapAdapter, sushi::SushiAdapter, DexAdapter,
    },
    market_snapshot::{
        pool_state_store::{
            should_publish_clmm_to_redis, AquariusPoolStateValue, RedisPoolStateStore, XykPoolStateValue,
        },
        ClmmPoolSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    std::{
        collections::{HashMap, HashSet},
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Arc,
        },
    },
    tokio::sync::{mpsc, RwLock},
    tracing::{debug, info, warn},
};

/// Minimum reserve on either side (matches pool_state_publish dust filter).
const MIN_XYK_RESERVE_STROOPS: u128 = 100_000_000;

#[derive(Debug, Clone)]
pub enum FetchTask {
    SoroswapBatch { pool_addresses: Vec<String> },
    AquariusBatch { pool_addresses: Vec<String> },
    PhoenixBatch { pool_addresses: Vec<String> },
    CometPool { pool_address: String },
    ClmmPool { source: String, pool_address: String },
}

#[derive(Default)]
pub struct FetchPipelineMetrics {
    pub high_dropped: AtomicU64,
    pub tasks_completed: AtomicU64,
    pub tasks_failed: AtomicU64,
    pub redis_writes: AtomicU64,
    high_depth: AtomicUsize,
}

impl FetchPipelineMetrics {
    fn log_periodic_summary(&self) {
        info!(
            high_dropped = self.high_dropped.load(Ordering::Relaxed),
            tasks_completed = self.tasks_completed.load(Ordering::Relaxed),
            tasks_failed = self.tasks_failed.load(Ordering::Relaxed),
            redis_writes = self.redis_writes.load(Ordering::Relaxed),
            high_queue_depth = self.high_depth.load(Ordering::Relaxed),
            "fetch pipeline stats"
        );
    }
}

#[derive(Debug)]
enum PoolStateUpdate {
    Xyk(Vec<XykPoolStateValue>),
    Aquarius(Vec<AquariusPoolStateValue>),
    Clmm(ClmmPoolSnapshot),
}

pub struct FetchPipelineConfig {
    pub worker_count: usize,
    pub refresh_concurrency: usize,
    pub high_queue_capacity: usize,
}

impl FetchPipelineConfig {
    pub fn from_env(refresh_concurrency: usize) -> Self {
        Self {
            worker_count: std::env::var("FETCH_WORKER_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8)
                .max(1),
            refresh_concurrency,
            high_queue_capacity: std::env::var("FETCH_HIGH_QUEUE_CAPACITY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512)
                .max(64),
        }
    }
}

pub fn fetch_pipeline_enabled_from_env() -> bool {
    std::env::var("FETCH_PIPELINE_ENABLED")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true)
}

#[derive(Clone)]
pub struct FetchPipelineHandle {
    high_tx: mpsc::Sender<FetchTask>,
    metrics: Arc<FetchPipelineMetrics>,
}

impl FetchPipelineHandle {
    pub fn enqueue_touched(&self, touched: HashSet<PoolRef>) {
        for pool in touched {
            let task = match pool.source.as_str() {
                "soroswap" => FetchTask::SoroswapBatch {
                    pool_addresses: vec![pool.pool_address],
                },
                "aquarius" => FetchTask::AquariusBatch {
                    pool_addresses: vec![pool.pool_address],
                },
                "phoenix" => FetchTask::PhoenixBatch {
                    pool_addresses: vec![pool.pool_address],
                },
                "comet" => FetchTask::CometPool {
                    pool_address: pool.pool_address,
                },
                "sushi" | "aquarius_clmm" => FetchTask::ClmmPool {
                    source: pool.source,
                    pool_address: pool.pool_address,
                },
                other => {
                    debug!(source = other, pool = %pool.pool_address, "ledger touch: no fetch handler");
                    continue;
                }
            };
            match self.high_tx.try_send(task) {
                Ok(()) => {
                    self.metrics.high_depth.fetch_add(1, Ordering::Relaxed);
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.metrics.high_dropped.fetch_add(1, Ordering::Relaxed);
                    warn!("fetch pipeline high queue full, dropping ledger task");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    warn!("fetch pipeline high queue closed");
                }
            }
        }
    }
}

struct FetchWorkerContext {
    rpc: Arc<SorobanRpc>,
    soroswap: Arc<SoroswapAdapter>,
    aquarius: Arc<AquariusAdapter>,
    phoenix: Arc<PhoenixAdapter>,
    comet: Arc<CometAdapter>,
    sushi: Arc<SushiAdapter>,
    aquarius_clmm: Arc<AquariusClmmAdapter>,
    shared: Arc<RwLock<WorkerShared>>,
    refresh_concurrency: usize,
}

pub fn spawn_fetch_pipeline(
    config: FetchPipelineConfig,
    pool_store: Arc<RedisPoolStateStore>,
    rpc: Arc<SorobanRpc>,
    shared: Arc<RwLock<WorkerShared>>,
    soroswap: Arc<SoroswapAdapter>,
    aquarius: Arc<AquariusAdapter>,
    phoenix: Arc<PhoenixAdapter>,
    comet: Arc<CometAdapter>,
    sushi: Arc<SushiAdapter>,
    aquarius_clmm: Arc<AquariusClmmAdapter>,
) -> FetchPipelineHandle {
    let (high_tx, mut high_rx) = mpsc::channel::<FetchTask>(config.high_queue_capacity);
    let (redis_tx, mut redis_rx) = mpsc::channel(config.high_queue_capacity.max(1024));

    let metrics = Arc::new(FetchPipelineMetrics::default());
    let stats_metrics = metrics.clone();

    let worker_ctx = Arc::new(FetchWorkerContext {
        rpc,
        soroswap,
        aquarius,
        phoenix,
        comet,
        sushi,
        aquarius_clmm,
        shared,
        refresh_concurrency: config.refresh_concurrency,
    });

    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.worker_count));
    let dispatch_ctx = worker_ctx.clone();
    let dispatch_redis = redis_tx.clone();
    let dispatch_metrics = metrics.clone();
    tokio::spawn(async move {
        while let Some(task) = high_rx.recv().await {
            dispatch_metrics.high_depth.fetch_sub(1, Ordering::Relaxed);

            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let ctx = dispatch_ctx.clone();
            let redis_tx = dispatch_redis.clone();
            let dispatch_metrics = dispatch_metrics.clone();
            tokio::spawn(async move {
                let _permit = permit;
                match execute_fetch_task(ctx.as_ref(), task).await {
                    Ok(updates) => {
                        for update in updates {
                            if redis_tx.send(update).await.is_err() {
                                return;
                            }
                        }
                        dispatch_metrics.tasks_completed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        dispatch_metrics.tasks_failed.fetch_add(1, Ordering::Relaxed);
                        warn!(%error, "fetch task failed");
                    }
                }
            });
        }
    });

    let pool_store_sink = pool_store.clone();
    let redis_metrics = metrics.clone();
    tokio::spawn(async move {
        while let Some(update) = redis_rx.recv().await {
            let result = match update {
                PoolStateUpdate::Xyk(values) if !values.is_empty() => pool_store_sink.set_xyk_batch(&values).await,
                PoolStateUpdate::Aquarius(values) if !values.is_empty() => {
                    pool_store_sink.set_aquarius_batch(&values).await
                }
                PoolStateUpdate::Clmm(snapshot) => pool_store_sink.set_clmm_batch(&[snapshot]).await,
                _ => Ok(()),
            };
            if let Err(error) = result {
                warn!(%error, "redis pool state write failed");
            } else {
                redis_metrics.redis_writes.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let stats_interval_secs = std::env::var("FETCH_STATS_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
        .max(15);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(stats_interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            stats_metrics.log_periodic_summary();
        }
    });

    info!(
        worker_count = config.worker_count,
        refresh_concurrency = config.refresh_concurrency,
        "Fetch pipeline started (ledger touched → RPC workers → Redis)"
    );

    FetchPipelineHandle { high_tx, metrics }
}

async fn execute_fetch_task(ctx: &FetchWorkerContext, task: FetchTask) -> Result<Vec<PoolStateUpdate>> {
    match task {
        FetchTask::SoroswapBatch { pool_addresses } => {
            let sources = ctx.shared.read().await.sources.clone();
            let results =
                batch_refresh_soroswap_reserves_parallel(ctx.rpc.as_ref(), &pool_addresses, ctx.refresh_concurrency)
                    .await?;
            ctx.soroswap.apply_batch_reserves(&results).await;
            let values = xyk_values_from_batch(&sources, "soroswap", &results);
            Ok(if values.is_empty() {
                vec![]
            } else {
                vec![PoolStateUpdate::Xyk(values)]
            })
        }
        FetchTask::AquariusBatch { pool_addresses } => {
            ctx.aquarius.refresh_pool_addresses(&pool_addresses).await?;
            let states = ctx.aquarius.export_pool_quote_states_for(&pool_addresses).await;
            let values: Vec<AquariusPoolStateValue> = states
                .into_iter()
                .map(|state| AquariusPoolStateValue {
                    pool_address: state.pool_address,
                    tokens: state.tokens,
                    reserves: state.reserves,
                    fee_bps: state.fee_bps,
                    is_stable: state.is_stable,
                    amp: state.amp,
                })
                .collect();
            Ok(if values.is_empty() {
                vec![]
            } else {
                vec![PoolStateUpdate::Aquarius(values)]
            })
        }
        FetchTask::PhoenixBatch { pool_addresses } => {
            ctx.phoenix.refresh_touched_pools(&pool_addresses).await?;
            let sources = ctx.shared.read().await.sources.clone();
            let values =
                collect_xyk_from_adapter_cache(&sources, "phoenix", &pool_addresses, ctx.phoenix.as_ref()).await;
            Ok(if values.is_empty() {
                vec![]
            } else {
                vec![PoolStateUpdate::Xyk(values)]
            })
        }
        FetchTask::CometPool { pool_address } => {
            if !ctx.comet.refresh_pool(&pool_address).await? {
                return Ok(vec![]);
            }
            let sources = ctx.shared.read().await.sources.clone();
            let values = collect_xyk_from_adapter_cache(
                &sources,
                "comet",
                std::slice::from_ref(&pool_address),
                ctx.comet.as_ref(),
            )
            .await;
            Ok(if values.is_empty() {
                vec![]
            } else {
                vec![PoolStateUpdate::Xyk(values)]
            })
        }
        FetchTask::ClmmPool { source, pool_address } => {
            match source.as_str() {
                "sushi" => ctx.sushi.ensure_pool_loaded(&pool_address).await?,
                "aquarius_clmm" => ctx.aquarius_clmm.ensure_pool_loaded(&pool_address).await?,
                other => {
                    anyhow::bail!("unknown CLMM source {}", other);
                }
            }

            let exported = match source.as_str() {
                "sushi" => ctx.sushi.export_clmm_snapshots().await,
                "aquarius_clmm" => ctx.aquarius_clmm.export_clmm_snapshots().await,
                _ => Vec::new(),
            };
            let Some(snapshot) = exported
                .into_iter()
                .find(|s| s.pool_address == pool_address && should_publish_clmm_to_redis(s))
            else {
                return Ok(vec![]);
            };

            let mut guard = ctx.shared.write().await;
            if let Some(existing) = guard
                .clmm_pools
                .iter_mut()
                .find(|p| p.source == snapshot.source && p.pool_address == snapshot.pool_address)
            {
                *existing = snapshot.clone();
            } else {
                guard.clmm_pools.push(snapshot.clone());
            }

            Ok(vec![PoolStateUpdate::Clmm(snapshot)])
        }
    }
}

fn xyk_values_from_batch(
    sources: &[SourceSnapshot],
    source: &str,
    results: &[(String, Option<(u128, u128)>)],
) -> Vec<XykPoolStateValue> {
    let Some(source_snapshot) = sources.iter().find(|s| s.source == source) else {
        return Vec::new();
    };
    let topology: HashMap<String, &TradingPairSnapshot> = source_snapshot
        .pairs
        .iter()
        .map(|p| (p.pool_address.clone(), p))
        .collect();

    let mut out = Vec::new();
    for (addr, reserves) in results {
        let Some((r0, r1)) = reserves else {
            continue;
        };
        if *r0 == 0 || *r1 == 0 || *r0 < MIN_XYK_RESERVE_STROOPS || *r1 < MIN_XYK_RESERVE_STROOPS {
            continue;
        }
        let Some(pair) = topology.get(addr) else {
            continue;
        };
        out.push(XykPoolStateValue::new(
            source,
            addr,
            &pair.token_a,
            &pair.token_b,
            pair.fee_bps,
            *r0,
            *r1,
        ));
    }
    out
}

async fn collect_xyk_from_adapter_cache(
    sources: &[SourceSnapshot],
    source: &str,
    pool_addresses: &[String],
    adapter: &dyn DexAdapter,
) -> Vec<XykPoolStateValue> {
    let wanted: HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
    let topology: HashMap<String, &TradingPairSnapshot> = sources
        .iter()
        .find(|s| s.source == source)
        .into_iter()
        .flat_map(|s| &s.pairs)
        .map(|p| (p.pool_address.clone(), p))
        .collect();

    let mut out = Vec::new();
    for pair in adapter.get_cached_pairs().await {
        if !wanted.contains(pair.pool_address.as_str()) {
            continue;
        }
        let (Some(reserve_a), Some(reserve_b)) = (pair.reserve_a, pair.reserve_b) else {
            continue;
        };
        if reserve_a == 0 ||
            reserve_b == 0 ||
            reserve_a < MIN_XYK_RESERVE_STROOPS ||
            reserve_b < MIN_XYK_RESERVE_STROOPS
        {
            continue;
        }
        let Some(topo) = topology.get(&pair.pool_address) else {
            continue;
        };
        out.push(XykPoolStateValue::new(
            source,
            &pair.pool_address,
            &topo.token_a,
            &topo.token_b,
            topo.fee_bps,
            reserve_a,
            reserve_b,
        ));
    }
    out
}

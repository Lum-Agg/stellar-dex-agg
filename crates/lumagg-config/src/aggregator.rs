use {
    crate::{set, set_list, set_option},
    anyhow::{bail, Result},
    serde::Deserialize,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregatorConfig {
    pub network: Network,
    pub redis: Option<Redis>,
    #[serde(default)]
    pub worker: Worker,
    #[serde(default)]
    pub api: Api,
    #[serde(default)]
    pub routing: Routing,
    #[serde(default)]
    pub dex: Dex,
    #[serde(default)]
    pub access: Access,
    #[serde(default)]
    pub features: Features,
    #[serde(default)]
    pub monitoring: Monitoring,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub rpc_url: String,
    #[serde(default = "mainnet_passphrase")]
    pub passphrase: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Redis {
    pub url: String,
    #[serde(default = "redis_channel")]
    pub channel: String,
    #[serde(default = "ten")]
    pub keep_latest: usize,
    #[serde(default = "thousand")]
    pub poll_interval_ms: u64,
    #[serde(default = "day")]
    pub pool_state_ttl_secs: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Worker {
    pub enabled_dex_sources: Option<Vec<String>>,
    pub discovery_interval_secs: Option<u64>,
    pub refresh_interval_secs: Option<u64>,
    pub pool_publish_interval_secs: Option<u64>,
    pub pool_state_refresh_concurrency: Option<usize>,
    pub ledger_watcher_enabled: Option<bool>,
    pub ledger_poll_secs: Option<f64>,
    pub ledger_max_catchup: Option<u32>,
    pub ledger_max_touched_refresh: Option<usize>,
    pub fetch_pipeline_enabled: Option<bool>,
    pub fetch_worker_count: Option<usize>,
    pub fetch_high_queue_capacity: Option<usize>,
    pub fetch_stats_interval_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Api {
    pub listen_addr: Option<String>,
    pub aggregator_contract: Option<String>,
    pub token_logo_dir: Option<String>,
    pub token_logo_base_url: Option<String>,
    pub token_logo_list_urls: Option<Vec<String>>,
    pub instruction_leeway: Option<u64>,
    pub quote_on_chain_validate: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Routing {
    pub split_threshold_bps: Option<u32>,
    pub split_competitive_delta_bps: Option<u32>,
    pub min_split_fraction_bps: Option<u32>,
    pub max_splits: Option<usize>,
    pub max_hops: Option<usize>,
    pub max_multi_hop_paths: Option<usize>,
    pub max_direct_paths: Option<usize>,
    pub quote_rpc_hydrate_enabled: Option<bool>,
    pub quote_hydrate_max_pools: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dex {
    pub aquarius_hydrate_concurrency: Option<usize>,
    pub horizon_url: Option<String>,
    pub soroswap_factory_contract: Option<String>,
    pub comet_factory: Option<String>,
    pub comet_extra_pools: Option<Vec<String>>,
    pub comet_factory_events_ledger_window: Option<u32>,
    pub sushi_discovery_rpc: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Access {
    pub partner_api_keys: Option<Vec<String>>,
    pub rate_limit_bypass_ips: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Features {
    pub escrow_contract: Option<String>,
    pub indexer_db_path: Option<String>,
    pub price_db_path: Option<String>,
    pub price_sampler_enabled: Option<bool>,
    pub price_sample_secs: Option<u64>,
    pub price_sample_token_limit: Option<usize>,
    pub price_retention_days: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Monitoring {
    pub log_filter: Option<String>,
    pub telegram_enabled: Option<bool>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub telegram_primary_api_port: Option<u16>,
    pub telegram_heartbeat_interval_secs: Option<u64>,
    pub api_health_url: Option<String>,
    pub mainnet_rpc_ref_url: Option<String>,
    pub quote_redis_miss_alert_min: Option<usize>,
    pub quote_redis_miss_alert_ratio_bps: Option<usize>,
}

impl AggregatorConfig {
    pub fn validate_embedded(&self) -> Result<()> {
        if self.network.rpc_url.trim().is_empty() {
            bail!("network.rpc_url must not be empty");
        }
        Ok(())
    }

    pub fn validate_cluster(&self) -> Result<()> {
        self.validate_embedded()?;
        let Some(redis) = &self.redis else {
            bail!("redis section is required for cluster mode");
        };
        if redis.url.trim().is_empty() {
            bail!("redis.url must not be empty");
        }
        if redis.keep_latest == 0 {
            bail!("redis.keep_latest must be greater than zero");
        }
        Ok(())
    }

    pub fn apply(&self) {
        set("LUMAGG_MODE", "cluster");
        set("RPC_URL", &self.network.rpc_url);
        set("NETWORK_PASSPHRASE", &self.network.passphrase);
        if let Some(redis) = &self.redis {
            set("SNAPSHOT_BACKEND", "redis");
            set("SNAPSHOT_REDIS_URL", &redis.url);
            set("SNAPSHOT_REDIS_CHANNEL", &redis.channel);
            set("SNAPSHOT_REDIS_KEEP_LATEST", redis.keep_latest);
            set("SNAPSHOT_POLL_INTERVAL_MS", redis.poll_interval_ms);
            set("POOL_STATE_TTL_SECS", redis.pool_state_ttl_secs);
        }

        set_list("ENABLED_DEX_SOURCES", &self.worker.enabled_dex_sources);
        set_option("DISCOVERY_INTERVAL_SECS", &self.worker.discovery_interval_secs);
        set_option("REFRESH_INTERVAL_SECS", &self.worker.refresh_interval_secs);
        set_option("POOL_PUBLISH_INTERVAL_SECS", &self.worker.pool_publish_interval_secs);
        set_option(
            "POOL_STATE_REFRESH_CONCURRENCY",
            &self.worker.pool_state_refresh_concurrency,
        );
        set_option("LEDGER_WATCHER_ENABLED", &self.worker.ledger_watcher_enabled);
        set_option("LEDGER_POLL_SECS", &self.worker.ledger_poll_secs);
        set_option("LEDGER_MAX_CATCHUP", &self.worker.ledger_max_catchup);
        set_option("LEDGER_MAX_TOUCHED_REFRESH", &self.worker.ledger_max_touched_refresh);
        set_option("FETCH_PIPELINE_ENABLED", &self.worker.fetch_pipeline_enabled);
        set_option("FETCH_WORKER_COUNT", &self.worker.fetch_worker_count);
        set_option("FETCH_HIGH_QUEUE_CAPACITY", &self.worker.fetch_high_queue_capacity);
        set_option("FETCH_STATS_INTERVAL_SECS", &self.worker.fetch_stats_interval_secs);

        set_option("LISTEN_ADDR", &self.api.listen_addr);
        set_option("AGGREGATOR_CONTRACT", &self.api.aggregator_contract);
        set_option("TOKEN_LOGO_DIR", &self.api.token_logo_dir);
        set_option("TOKEN_LOGO_BASE_URL", &self.api.token_logo_base_url);
        set_list("TOKEN_LOGO_LIST_URLS", &self.api.token_logo_list_urls);
        set_option("INSTRUCTION_LEEWAY", &self.api.instruction_leeway);
        set_option("QUOTE_ON_CHAIN_VALIDATE", &self.api.quote_on_chain_validate);

        set_option("SPLIT_THRESHOLD_BPS", &self.routing.split_threshold_bps);
        set_option("SPLIT_COMPETITIVE_DELTA_BPS", &self.routing.split_competitive_delta_bps);
        set_option("MIN_SPLIT_FRACTION_BPS", &self.routing.min_split_fraction_bps);
        set_option("MAX_SPLITS", &self.routing.max_splits);
        set_option("PATH_FINDER_MAX_HOPS", &self.routing.max_hops);
        set_option("PATH_FINDER_MAX_MULTI_HOP_PATHS", &self.routing.max_multi_hop_paths);
        set_option("PATH_FINDER_MAX_DIRECT_PATHS", &self.routing.max_direct_paths);
        set_option("QUOTE_RPC_HYDRATE_ENABLED", &self.routing.quote_rpc_hydrate_enabled);
        set_option("QUOTE_HYDRATE_MAX_POOLS", &self.routing.quote_hydrate_max_pools);

        set_option("AQUARIUS_HYDRATE_CONCURRENCY", &self.dex.aquarius_hydrate_concurrency);
        set_option("HORIZON_URL", &self.dex.horizon_url);
        set_option("SOROSWAP_FACTORY_CONTRACT", &self.dex.soroswap_factory_contract);
        set_option("COMET_FACTORY", &self.dex.comet_factory);
        set_list("COMET_EXTRA_POOLS", &self.dex.comet_extra_pools);
        set_option(
            "COMET_FACTORY_EVENTS_LEDGER_WINDOW",
            &self.dex.comet_factory_events_ledger_window,
        );
        set_option("SUSHI_DISCOVERY_RPC", &self.dex.sushi_discovery_rpc);

        set_list("LUMAGG_PARTNER_API_KEYS", &self.access.partner_api_keys);
        set_list("QUOTE_RATE_LIMIT_BYPASS_IPS", &self.access.rate_limit_bypass_ips);
        set_option("ESCROW_CONTRACT", &self.features.escrow_contract);
        set_option("INDEXER_DB_PATH", &self.features.indexer_db_path);
        set_option("PRICE_DB_PATH", &self.features.price_db_path);
        if let Some(enabled) = self.features.price_sampler_enabled {
            set("PRICE_SAMPLER", if enabled { "1" } else { "0" });
        }
        set_option("PRICE_SAMPLE_SECS", &self.features.price_sample_secs);
        set_option("PRICE_SAMPLE_TOKEN_LIMIT", &self.features.price_sample_token_limit);
        set_option("PRICE_RETENTION_DAYS", &self.features.price_retention_days);

        set_option("RUST_LOG", &self.monitoring.log_filter);
        set_option("TELEGRAM_ALERTS_ENABLED", &self.monitoring.telegram_enabled);
        set_option("TELEGRAM_BOT_TOKEN", &self.monitoring.telegram_bot_token);
        set_option("TELEGRAM_CHAT_ID", &self.monitoring.telegram_chat_id);
        set_option("TELEGRAM_PRIMARY_API_PORT", &self.monitoring.telegram_primary_api_port);
        set_option(
            "TELEGRAM_HEARTBEAT_INTERVAL_SECS",
            &self.monitoring.telegram_heartbeat_interval_secs,
        );
        set_option("MONITOR_API_HEALTH_URL", &self.monitoring.api_health_url);
        set_option("MAINNET_RPC_REF_URL", &self.monitoring.mainnet_rpc_ref_url);
        set_option(
            "QUOTE_REDIS_MISS_ALERT_MIN",
            &self.monitoring.quote_redis_miss_alert_min,
        );
        set_option(
            "QUOTE_REDIS_MISS_ALERT_RATIO_BPS",
            &self.monitoring.quote_redis_miss_alert_ratio_bps,
        );
    }
}

fn mainnet_passphrase() -> String {
    "Public Global Stellar Network ; September 2015".into()
}
fn redis_channel() -> String {
    "lumagg:snapshot:events".into()
}
fn ten() -> usize {
    10
}
fn thousand() -> u64 {
    1_000
}
fn day() -> u64 {
    86_400
}

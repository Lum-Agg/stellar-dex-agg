//! Public analytics stats from the analytics-indexer SQLite DB (Tranche 3
//! handoff).

use {
    crate::xlm_price::enrich_daily_with_historical_usd,
    analytics_indexer::{export, store::IndexStore},
    axum::{
        extract::Query,
        http::{header, StatusCode},
        response::{IntoResponse, Response},
        Json,
    },
    redis::AsyncCommands,
    serde::{Deserialize, Serialize},
    std::{
        fs,
        path::PathBuf,
        sync::OnceLock,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    },
    tokio::sync::RwLock,
};

const STATS_CACHE_TTL: Duration = Duration::from_secs(30);
const SHARED_STATS_CACHE_KEY: &str = "lumagg:api:stats:full:v2";
const SHARED_STATS_CACHE_FILE: &str = "/tmp/lumagg-stats-full-v2.json";

struct CachedStats {
    created_at: Instant,
    data: StatsData,
}

static STATS_CACHE: OnceLock<RwLock<Option<CachedStats>>> = OnceLock::new();

fn stats_cache() -> &'static RwLock<Option<CachedStats>> {
    STATS_CACHE.get_or_init(|| RwLock::new(None))
}

async fn read_shared_stats_cache() -> Option<serde_json::Value> {
    let redis_url = std::env::var("SNAPSHOT_REDIS_URL").ok()?;
    let client = redis::Client::open(redis_url).ok()?;
    let mut connection = client.get_multiplexed_async_connection().await.ok()?;
    let payload: Option<String> = connection.get(SHARED_STATS_CACHE_KEY).await.ok()?;
    serde_json::from_str(&payload?).ok()
}

async fn write_shared_stats_cache(response: &StatsResponse) {
    let Ok(redis_url) = std::env::var("SNAPSHOT_REDIS_URL") else {
        return;
    };
    let Ok(payload) = serde_json::to_string(response) else {
        return;
    };
    let Ok(client) = redis::Client::open(redis_url) else {
        return;
    };
    let Ok(mut connection) = client.get_multiplexed_async_connection().await else {
        return;
    };
    let _: redis::RedisResult<()> = connection.set_ex(SHARED_STATS_CACHE_KEY, payload, 15).await;
}

fn shared_stats_file_path() -> PathBuf {
    std::env::var_os("STATS_CACHE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SHARED_STATS_CACHE_FILE))
}

fn read_file_stats_cache() -> Option<serde_json::Value> {
    let payload = fs::read_to_string(shared_stats_file_path()).ok()?;
    let cache: serde_json::Value = serde_json::from_str(&payload).ok()?;
    if cache.get("schema_version")?.as_u64()? != 2 {
        return None;
    }
    let created_at = cache.get("created_at")?.as_u64()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if now.saturating_sub(created_at) > 15 {
        return None;
    }
    cache.get("response").cloned()
}

fn write_file_stats_cache(response: &StatsResponse) {
    let path = shared_stats_file_path();
    let cache = serde_json::json!({
        "schema_version": 2,
        "created_at": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        "response": response,
    });
    let Ok(payload) = serde_json::to_vec(&cache) else {
        return;
    };
    let temporary = path.with_extension(format!("json.{}", std::process::id()));
    if fs::write(&temporary, payload).is_ok() {
        let _ = fs::rename(temporary, path);
    }
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// UTC day `YYYY-MM-DD`; omit for full rollup + indexer summary.
    pub day: Option<String>,
    /// `csv` for grant report download; default JSON.
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<StatsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsData {
    pub db_path: String,
    pub invocation_count: i64,
    pub cursor_ledger: Option<u32>,
    pub oldest_created_at: Option<i64>,
    pub daily: Vec<export::DailyStats>,
    /// How USD notional was priced (when enrichment succeeded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd_pricing: Option<&'static str>,
}

fn indexer_db_path() -> Option<String> {
    std::env::var("INDEXER_DB_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("LUMAGG_INDEXER_DB_PATH").ok().filter(|s| !s.is_empty()))
}

fn activity_counts(functions: &std::collections::BTreeMap<String, u64>, total: u64) -> (u64, u64, u64) {
    let swaps = functions.get("swap").copied().unwrap_or(0);
    let arbitrage = functions.get("round_trip_swap").copied().unwrap_or(0);
    (swaps, arbitrage, total.saturating_sub(swaps.saturating_add(arbitrage)))
}

fn activity_share(count: u64, total: u64) -> String {
    if total == 0 {
        String::new()
    } else {
        format!("{:.6}", count as f64 / total as f64 * 100.0)
    }
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn stats_csv(daily: &[export::DailyStats]) -> Result<String, serde_json::Error> {
    // Schema v2 deliberately removes mixed-token sums labelled as stroops.
    let mut lines = vec![
        "day,tx_count,unique_caller_addresses,entry_amounts_by_token_raw,notional_in_usd_priced_subtotal,routed_amounts_by_token_raw,routed_dex_volume_usd_priced_subtotal,actual_amount_leg_count,routed_priced_leg_count,routed_pricing_coverage,round_trips_with_return_amount,round_trip_gross_surplus_usd_priced_subtotal,xlm_usd_daily_reference,split_swap_count,success_count,failed_count,user_swap_count,arbitrage_count,other_success_count,user_swap_share_pct,arbitrage_share_pct,observed_dex_leg_count,csv_schema_version".to_string(),
    ];
    let usd = |v: Option<f64>| {
        v.filter(|v| v.is_finite())
            .map(|v| format!("{v:.6}"))
            .unwrap_or_default()
    };
    for d in daily {
        let (swaps, arbitrage, other) = activity_counts(&d.by_function, d.tx_count);
        // Decimal strings retain exact raw i128 amounts in spreadsheet/JS tools.
        let entry: Vec<_> = d
            .by_token
            .iter()
            .map(|v| {
                serde_json::json!({
                    "token": v.token, "amount_in_raw": v.amount_in.to_string(),
                })
            })
            .collect();
        let routed: Vec<_> = d
            .routed_by_token
            .iter()
            .map(|v| {
                serde_json::json!({
                    "token": v.token, "amount_in_raw": v.routed_volume.to_string(),
                    "leg_count": v.routed_leg_count,
                })
            })
            .collect();
        let cells = [
            d.day.clone(),
            d.tx_count.to_string(),
            d.unique_users.to_string(),
            serde_json::to_string(&entry)?,
            usd(d.total_amount_in_usd),
            serde_json::to_string(&routed)?,
            usd(d.total_routed_dex_volume_usd),
            d.routed_leg_count.to_string(),
            d.routed_priced_leg_count.to_string(),
            usd(d.routed_pricing_coverage),
            d.round_trip_count.to_string(),
            usd(d.round_trip_gross_surplus_usd),
            d.xlm_usd
                .filter(|v| v.is_finite())
                .map(|v| format!("{v:.8}"))
                .unwrap_or_default(),
            d.split_swap_count.to_string(),
            d.success_count.to_string(),
            d.failed_count.to_string(),
            swaps.to_string(),
            arbitrage.to_string(),
            other.to_string(),
            activity_share(swaps, d.tx_count),
            activity_share(arbitrage, d.tx_count),
            d.by_dex.values().sum::<u64>().to_string(),
            "2".into(),
        ];
        lines.push(cells.iter().map(|v| csv_cell(v)).collect::<Vec<_>>().join(","));
    }
    Ok(lines.join("\n") + "\n")
}

pub async fn get_stats(Query(params): Query<StatsQuery>) -> Response {
    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(StatsResponse {
                success: false,
                data: None,
                error: Some("Analytics DB not configured (set INDEXER_DB_PATH on api-server)".into()),
            }),
        )
            .into_response();
    };

    // The dashboard requests the full JSON rollup repeatedly. Keep a short
    // process-local snapshot so multiple page loads do not rerun all daily
    // SQLite aggregates and historical price enrichment.
    let cacheable = params.day.is_none() && params.format.is_none();
    if cacheable {
        if let Some(cached) = stats_cache().read().await.as_ref() {
            if cached.created_at.elapsed() < STATS_CACHE_TTL {
                return (
                    StatusCode::OK,
                    [(header::CACHE_CONTROL, "public, max-age=15")],
                    Json(StatsResponse {
                        success: true,
                        data: Some(cached.data.clone()),
                        error: None,
                    }),
                )
                    .into_response();
            }
        }
        if let Some(cached) = read_file_stats_cache() {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=15, stale-while-revalidate=60")],
                Json(cached),
            )
                .into_response();
        }
        if let Some(cached) = read_shared_stats_cache().await {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "public, max-age=15, stale-while-revalidate=60")],
                Json(cached),
            )
                .into_response();
        }
    }

    let store = match IndexStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatsResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };

    let mut daily = if let Some(ref day) = params.day {
        match export::export_daily(&store, day) {
            Ok(one) => vec![one],
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(StatsResponse {
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                    }),
                )
                    .into_response();
            }
        }
    } else {
        match export::export_all_days(&store) {
            Ok(all) => all,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(StatsResponse {
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                    }),
                )
                    .into_response();
            }
        }
    };

    enrich_daily_with_historical_usd(&mut daily).await;
    let usd_pricing = daily
        .iter()
        .any(|d| d.total_amount_in_usd.is_some() || d.round_trip_gross_surplus_usd.is_some())
        .then_some("per_token_historical_usd_daily");

    let invocation_count = store.count_invocations().unwrap_or(0);
    let cursor_ledger = store.cursor_ledger().ok().flatten();
    let oldest_created_at = store.oldest_created_at().ok().flatten();

    if params.format.as_deref() == Some("csv") {
        let body = match stats_csv(&daily) {
            Ok(body) => body,
            Err(error) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("CSV serialization: {error}")).into_response()
            }
        };
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"lumagg-stats.csv\""),
            ],
            body,
        )
            .into_response();
    }

    let data = StatsData {
        db_path,
        invocation_count,
        cursor_ledger,
        oldest_created_at,
        daily,
        usd_pricing,
    };

    if cacheable {
        *stats_cache().write().await = Some(CachedStats {
            created_at: Instant::now(),
            data: data.clone(),
        });
    }

    let response = StatsResponse {
        success: true,
        data: Some(data),
        error: None,
    };
    if cacheable {
        write_shared_stats_cache(&response).await;
        write_file_stats_cache(&response);
    }

    (
        StatusCode::OK,
        if cacheable {
            Some([(header::CACHE_CONTROL, "public, max-age=15, stale-while-revalidate=60")])
        } else {
            None
        },
        Json(response),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_v2_retains_token_units_and_missing_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("stats.db")).unwrap();
        let mut row = export::export_daily(&store, "2026-10-29").unwrap();
        row.tx_count = 3;
        row.success_count = 3;
        row.unique_users = 2;
        row.by_function.insert("swap".into(), 1);
        row.by_function.insert("round_trip_swap".into(), 1);
        row.by_function.insert("unknown".into(), 1);
        row.by_dex.insert("aquarius".into(), 7);
        // A mixed-unit sum must never appear in the CSV as a currency amount.
        row.total_amount_in = 999;
        row.total_routed_dex_volume = 888;
        row.by_token.push(export::TokenVolume {
            token: "TOKEN".into(),
            amount_in: i128::MAX,
            routed_volume: 0,
            routed_leg_count: 0,
        });
        let csv = stats_csv(&[row]).unwrap();
        let header = csv.lines().next().unwrap();
        assert_eq!(header.split(',').count(), 23);
        assert!(!header.contains("stroops"));
        assert!(!header.contains("unique_users"));
        let line = csv.lines().nth(1).unwrap();
        assert_eq!(
            line,
            concat!(
                "2026-10-29,3,2,\"[{\"\"amount_in_raw\"\":\"\"170141183460469231731687303715884105727\"\",",
                "\"\"token\"\":\"\"TOKEN\"\"}]\",,[],,0,0,,0,,,0,3,0,1,1,1,33.333333,33.333333,7,2"
            )
        );
    }

    #[test]
    fn csv_quotes_commas_quotes_and_newlines() {
        assert_eq!(csv_cell("simple"), "simple");
        assert_eq!(csv_cell("a,\"b\"\nc"), "\"a,\"\"b\"\"\nc\"");
        assert_eq!(stats_csv(&[]).unwrap().lines().count(), 1);
    }

    #[test]
    fn activity_split_uses_successful_functions_and_keeps_other_calls_separate() {
        let functions = [
            ("swap".to_string(), 2),
            ("round_trip_swap".to_string(), 97),
            ("future".to_string(), 1),
        ]
        .into_iter()
        .collect();
        assert_eq!(activity_counts(&functions, 100), (2, 97, 1));
        assert_eq!(activity_share(2, 100), "2.000000");
        assert_eq!(activity_share(97, 100), "97.000000");
    }

    #[test]
    fn empty_activity_has_no_percentage() {
        assert_eq!(activity_counts(&Default::default(), 0), (0, 0, 0));
        assert_eq!(activity_share(0, 0), "");
    }
}

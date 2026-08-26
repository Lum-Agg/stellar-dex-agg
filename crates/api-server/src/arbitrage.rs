//! Public successful round-trip (arbitrage) history from analytics-indexer.

use {
    analytics_indexer::store::IndexStore,
    axum::{
        extract::Query,
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc},
    serde::{Deserialize, Serialize},
    std::collections::BTreeMap,
};

const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

#[derive(Debug, Deserialize)]
pub struct ArbitrageQuery {
    pub limit: Option<u32>,
    /// Opaque cursor from a previous page (`{created_at}:{tx_hash}`).
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArbitrageResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ArbitrageData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArbitrageData {
    pub round_trips: Vec<RoundTripItem>,
    /// Terminal statuses observed by the analytics indexer. These counts do
    /// not include bot broadcasts that have not been indexed yet.
    pub success_count: u64,
    pub failed_count: u64,
    /// Failed round trips classified from on-chain `resultXdr`.
    pub failure_reasons: Vec<FailureReasonCount>,
    /// Failed round trips whose result XDR was unavailable or could not be
    /// classified.
    pub unclassified_failed_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArbitrageStatsQuery {
    /// `hour`, `day`, `week`, or `month`.
    pub granularity: Option<String>,
    /// Unix seconds, inclusive. Defaults to a useful window per granularity.
    pub start: Option<i64>,
    /// Unix seconds, exclusive. Defaults to now.
    pub end: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ArbitrageStatsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ArbitrageStatsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArbitrageStatsData {
    pub granularity: String,
    pub start: i64,
    pub end: i64,
    pub buckets: Vec<ArbitrageStatsBucket>,
}

#[derive(Debug, Serialize)]
pub struct ArbitrageStatsBucket {
    pub start: i64,
    pub label: String,
    pub tx_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    pub xlm_tx_count: u64,
    pub usdc_tx_count: u64,
    pub xlm_surplus: String,
    pub usdc_surplus: String,
}

#[derive(Default)]
struct BucketTotals {
    tx_count: u64,
    success_count: u64,
    failed_count: u64,
    xlm_tx_count: u64,
    usdc_tx_count: u64,
    xlm_surplus: i128,
    usdc_surplus: i128,
}

fn bucket_start(ts: i64, granularity: &str) -> Option<DateTime<Utc>> {
    let dt = Utc.timestamp_opt(ts, 0).single()?;
    let date = dt.date_naive();
    let day = match granularity {
        "hour" => dt
            .with_minute(0)?
            .with_second(0)?
            .with_nanosecond(0)?,
        "day" => date.and_hms_opt(0, 0, 0)?.and_utc(),
        "week" => (date - Duration::days(i64::from(date.weekday().num_days_from_monday())))
            .and_hms_opt(0, 0, 0)?
            .and_utc(),
        "month" => date.with_day(1)?.and_hms_opt(0, 0, 0)?.and_utc(),
        _ => return None,
    };
    Some(day)
}

fn bucket_label(dt: DateTime<Utc>, granularity: &str) -> String {
    match granularity {
        "hour" => dt.format("%Y-%m-%d %H:00 UTC").to_string(),
        "day" => dt.format("%Y-%m-%d").to_string(),
        "week" => format!("Week of {}", dt.format("%Y-%m-%d")),
        "month" => dt.format("%Y-%m").to_string(),
        _ => dt.to_rfc3339(),
    }
}

fn default_window(granularity: &str, end: i64) -> (i64, i64) {
    let seconds = match granularity {
        "hour" => 24 * 60 * 60,
        "day" => 30 * 24 * 60 * 60,
        "week" => 12 * 7 * 24 * 60 * 60,
        "month" => 365 * 24 * 60 * 60,
        _ => 30 * 24 * 60 * 60,
    };
    (end.saturating_sub(seconds), end)
}

#[derive(Debug, Serialize)]
pub struct FailureReasonCount {
    pub reason: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct RoundTripItem {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub base_token: Option<String>,
    pub bridge_token: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    /// `amount_out - amount_in` when both parse as integers; otherwise omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gross_surplus: Option<String>,
    pub is_split: bool,
}

fn indexer_db_path() -> Option<String> {
    std::env::var("INDEXER_DB_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("LUMAGG_INDEXER_DB_PATH").ok().filter(|s| !s.is_empty()))
}

fn encode_cursor(created_at: i64, tx_hash: &str) -> String {
    format!("{created_at}:{tx_hash}")
}

fn parse_cursor(raw: &str) -> Result<(i64, &str), String> {
    let (ts, hash) = raw
        .split_once(':')
        .ok_or_else(|| "cursor must be `{created_at}:{tx_hash}`".to_string())?;
    let created_at: i64 = ts
        .parse()
        .map_err(|_| "cursor created_at must be an integer timestamp".to_string())?;
    if hash.is_empty() || hash.len() > 128 {
        return Err("cursor tx_hash is empty or too long".into());
    }
    Ok((created_at, hash))
}

fn gross_surplus(amount_in: &str, amount_out: Option<&str>) -> Option<String> {
    let out = amount_out?;
    let ain: i128 = amount_in.parse().ok()?;
    let aout: i128 = out.parse().ok()?;
    Some((aout - ain).to_string())
}

pub async fn get_arbitrage(Query(params): Query<ArbitrageQuery>) -> Response {
    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ArbitrageResponse {
                success: false,
                data: None,
                error: Some("Analytics DB not configured (set INDEXER_DB_PATH on api-server)".into()),
            }),
        )
            .into_response();
    };

    let before = match params.cursor.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(raw) => match parse_cursor(raw) {
            Ok(v) => Some(v),
            Err(msg) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ArbitrageResponse {
                        success: false,
                        data: None,
                        error: Some(msg),
                    }),
                )
                    .into_response();
            }
        },
    };

    let limit = params.limit.unwrap_or(25).clamp(1, 50);

    let store = match IndexStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };

    let rows = match store.list_recent_round_trips(limit, before) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageResponse {
                    success: false,
                    data: None,
                    error: Some(format!("query round trips: {e}")),
                }),
            )
                .into_response();
        }
    };

    let (success_count, failed_count) = match store.round_trip_status_counts() {
        Ok(counts) => counts,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageResponse {
                    success: false,
                    data: None,
                    error: Some(format!("count round-trip statuses: {e}")),
                }),
            )
                .into_response();
        }
    };

    let failure_reasons: Vec<FailureReasonCount> = match store.round_trip_failure_reason_counts() {
        Ok(rows) => rows
            .into_iter()
            .map(|(reason, count)| FailureReasonCount { reason, count })
            .collect(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageResponse {
                    success: false,
                    data: None,
                    error: Some(format!("count round-trip failure reasons: {e}")),
                }),
            )
                .into_response();
        }
    };
    let classified_failed_count = failure_reasons.iter().map(|item| item.count).sum::<u64>();
    let unclassified_failed_count = failed_count.saturating_sub(classified_failed_count);

    let next_cursor = if rows.len() as u32 >= limit {
        rows.last().map(|r| encode_cursor(r.created_at, &r.tx_hash))
    } else {
        None
    };

    let round_trips = rows
        .into_iter()
        .map(|r| RoundTripItem {
            tx_hash: r.tx_hash,
            ledger: r.ledger,
            created_at: r.created_at,
            status: r.status,
            base_token: r.token_in,
            bridge_token: r.bridge_token,
            amount_in: r.amount_in.clone(),
            amount_out: r.amount_out.clone(),
            gross_surplus: gross_surplus(r.amount_in.as_str(), r.amount_out.as_deref()),
            is_split: r.is_split,
        })
        .collect();

    (
        StatusCode::OK,
        Json(ArbitrageResponse {
            success: true,
            data: Some(ArbitrageData {
                round_trips,
                success_count,
                failed_count,
                failure_reasons,
                unclassified_failed_count,
                next_cursor,
            }),
            error: None,
        }),
    )
        .into_response()
}

/// Time-bucketed arbitrage reporting. This endpoint is intentionally separate
/// from `/api/v1/stats`, whose daily schema is consumed by DefiLlama.
pub async fn get_arbitrage_stats(Query(params): Query<ArbitrageStatsQuery>) -> Response {
    let granularity = params
        .granularity
        .as_deref()
        .unwrap_or("day")
        .trim()
        .to_ascii_lowercase();
    if !matches!(granularity.as_str(), "hour" | "day" | "week" | "month") {
        return (
            StatusCode::BAD_REQUEST,
            Json(ArbitrageStatsResponse {
                success: false,
                data: None,
                error: Some("granularity must be hour, day, week, or month".into()),
            }),
        )
            .into_response();
    }

    let end = params.end.unwrap_or_else(|| Utc::now().timestamp());
    let (default_start, default_end) = default_window(&granularity, end);
    let start = params.start.unwrap_or(default_start);
    let end = params.end.unwrap_or(default_end);
    if start >= end {
        return (
            StatusCode::BAD_REQUEST,
            Json(ArbitrageStatsResponse {
                success: false,
                data: None,
                error: Some("start must be earlier than end".into()),
            }),
        )
            .into_response();
    }

    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ArbitrageStatsResponse {
                success: false,
                data: None,
                error: Some("Analytics DB not configured (set INDEXER_DB_PATH on api-server)".into()),
            }),
        )
            .into_response();
    };
    let store = match IndexStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageStatsResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };
    let rows = match store.list_round_trips_between(start, end) {
        Ok(rows) => rows,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ArbitrageStatsResponse {
                    success: false,
                    data: None,
                    error: Some(format!("query arbitrage stats: {e}")),
                }),
            )
                .into_response();
        }
    };

    let mut totals: BTreeMap<i64, BucketTotals> = BTreeMap::new();
    for row in rows {
        let Some(bucket) = bucket_start(row.created_at, &granularity) else {
            continue;
        };
        let bucket_ts = bucket.timestamp();
        let entry = totals.entry(bucket_ts).or_default();
        entry.tx_count = entry.tx_count.saturating_add(1);
        if row.status == "SUCCESS" {
            entry.success_count = entry.success_count.saturating_add(1);
            let surplus = row
                .amount_out
                .as_deref()
                .and_then(|out| out.parse::<i128>().ok())
                .zip(row.amount_in.parse::<i128>().ok())
                .map(|(out, input)| out.saturating_sub(input));
            match row.token_in.as_deref() {
                Some(XLM_SAC) => {
                    entry.xlm_tx_count = entry.xlm_tx_count.saturating_add(1);
                    if let Some(surplus) = surplus {
                        entry.xlm_surplus = entry.xlm_surplus.saturating_add(surplus);
                    }
                }
                Some(USDC_SAC) => {
                    entry.usdc_tx_count = entry.usdc_tx_count.saturating_add(1);
                    if let Some(surplus) = surplus {
                        entry.usdc_surplus = entry.usdc_surplus.saturating_add(surplus);
                    }
                }
                _ => {}
            }
        } else if row.status == "FAILED" {
            entry.failed_count = entry.failed_count.saturating_add(1);
        }
    }

    let buckets = totals
        .into_iter()
        .filter_map(|(start, totals)| {
            let dt = Utc.timestamp_opt(start, 0).single()?;
            Some(ArbitrageStatsBucket {
                start,
                label: bucket_label(dt, &granularity),
                tx_count: totals.tx_count,
                success_count: totals.success_count,
                failed_count: totals.failed_count,
                xlm_tx_count: totals.xlm_tx_count,
                usdc_tx_count: totals.usdc_tx_count,
                xlm_surplus: totals.xlm_surplus.to_string(),
                usdc_surplus: totals.usdc_surplus.to_string(),
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(ArbitrageStatsResponse {
            success: true,
            data: Some(ArbitrageStatsData {
                granularity,
                start,
                end,
                buckets,
            }),
            error: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::gross_surplus;

    #[test]
    fn surplus_is_out_minus_in() {
        assert_eq!(gross_surplus("10000000", Some("10005000")).as_deref(), Some("5000"));
        assert_eq!(gross_surplus("100", None), None);
        assert_eq!(gross_surplus("bad", Some("1")), None);
    }
}

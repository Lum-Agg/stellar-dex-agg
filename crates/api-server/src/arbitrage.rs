//! Public successful round-trip (arbitrage) history from analytics-indexer.

use {
    analytics_indexer::store::IndexStore,
    axum::{
        extract::Query,
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    serde::{Deserialize, Serialize},
};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
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

    let failure_reasons = match store.round_trip_failure_reason_counts() {
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
                next_cursor,
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

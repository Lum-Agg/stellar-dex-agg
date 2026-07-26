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
    serde::{Deserialize, Serialize},
};

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

#[derive(Debug, Serialize)]
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
        let mut lines = vec![
            "day,tx_count,unique_users,notional_in_stroops,notional_in_usd,routed_dex_volume_stroops,routed_dex_volume_usd,routed_leg_count,routed_priced_leg_count,routed_pricing_coverage,round_trip_count,round_trip_gross_surplus_usd,xlm_usd,split_swap_count,success_count,failed_count"
                .into(),
        ];
        for d in &daily {
            lines.push(format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                d.day,
                d.tx_count,
                d.unique_users,
                d.total_amount_in,
                d.total_amount_in_usd.map(|v| format!("{v:.6}")).unwrap_or_default(),
                d.total_routed_dex_volume,
                d.total_routed_dex_volume_usd
                    .map(|v| format!("{v:.6}"))
                    .unwrap_or_default(),
                d.routed_leg_count,
                d.routed_priced_leg_count,
                d.routed_pricing_coverage.map(|v| format!("{v:.6}")).unwrap_or_default(),
                d.round_trip_count,
                d.round_trip_gross_surplus_usd
                    .map(|v| format!("{v:.6}"))
                    .unwrap_or_default(),
                d.xlm_usd.map(|v| format!("{v:.8}")).unwrap_or_default(),
                d.split_swap_count,
                d.success_count,
                d.failed_count
            ));
        }
        let body = lines.join("\n") + "\n";
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

    (
        StatusCode::OK,
        Json(StatsResponse {
            success: true,
            data: Some(StatsData {
                db_path,
                invocation_count,
                cursor_ledger,
                oldest_created_at,
                daily,
                usd_pricing,
            }),
            error: None,
        }),
    )
        .into_response()
}

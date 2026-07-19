//! Latest and sampled USDC price endpoints.

use {
    crate::{price_mark::mark_token_usdc, price_store::PriceTick, state::AppState},
    axum::{
        extract::{Query, State},
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    serde::{Deserialize, Serialize},
    std::time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize)]
pub struct PricesQuery {
    pub ids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PriceHistoryQuery {
    pub id: Option<String>,
    pub range: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PricesResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<PricesData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PricesData {
    pub prices: Vec<PriceItem>,
}

#[derive(Debug, Serialize)]
pub struct PriceItem {
    pub id: String,
    pub price_usdc: f64,
    pub ts: i64,
    pub via: String,
}

#[derive(Debug, Serialize)]
pub struct PriceHistoryResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<PriceHistoryData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PriceHistoryData {
    pub id: String,
    pub range: String,
    pub points: Vec<PricePoint>,
}

#[derive(Debug, Serialize)]
pub struct PricePoint {
    pub ts: i64,
    pub price_usdc: f64,
}

pub async fn get_prices(State(state): State<AppState>, Query(params): Query<PricesQuery>) -> Response {
    let ids = match parse_ids(params.ids.as_deref()) {
        Ok(ids) => ids,
        Err(error) => return bad_request(error),
    };

    let mut prices = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(store) = &state.price_store {
            match store.latest(&id) {
                Ok(Some(tick)) => {
                    prices.push(price_item(tick));
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(token = %id, %error, "price store lookup failed");
                }
            }
        }

        let Some((price_usdc, via)) = mark_token_usdc(&state, &id).await else {
            continue;
        };
        let ts = unix_timestamp();
        if let Some(store) = &state.price_store {
            if let Err(error) = store.insert_tick(&id, ts, price_usdc, via) {
                tracing::warn!(token = %id, %error, "price store tick insert failed");
            }
        }
        prices.push(PriceItem {
            id,
            price_usdc,
            ts,
            via: via.to_string(),
        });
    }

    Json(PricesResponse {
        success: true,
        data: Some(PricesData { prices }),
        error: None,
    })
    .into_response()
}

pub async fn get_price_history(State(state): State<AppState>, Query(params): Query<PriceHistoryQuery>) -> Response {
    let Some(id) = params.id.as_deref().map(str::trim).filter(|id| !id.is_empty()) else {
        return history_bad_request("missing required query param: id");
    };
    let (range, seconds) = match parse_range(params.range.as_deref()) {
        Ok(range) => range,
        Err(error) => return history_bad_request(error),
    };

    let now = unix_timestamp();
    let points = match &state.price_store {
        Some(store) => match store.history(id, now.saturating_sub(seconds), now) {
            Ok(ticks) => ticks
                .into_iter()
                .map(|tick| PricePoint {
                    ts: tick.ts,
                    price_usdc: tick.price_usdc,
                })
                .collect(),
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PriceHistoryResponse {
                        success: false,
                        data: None,
                        error: Some(format!("read price history: {error}")),
                    }),
                )
                    .into_response();
            }
        },
        None => Vec::new(),
    };

    Json(PriceHistoryResponse {
        success: true,
        data: Some(PriceHistoryData {
            id: id.to_string(),
            range: range.to_string(),
            points,
        }),
        error: None,
    })
    .into_response()
}

fn parse_ids(ids: Option<&str>) -> Result<Vec<String>, &'static str> {
    let ids: Vec<_> = ids
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Err("missing required query param: ids");
    }
    if ids.len() > 50 {
        return Err("ids must contain at most 50 token ids");
    }
    Ok(ids)
}

fn parse_range(range: Option<&str>) -> Result<(&'static str, i64), &'static str> {
    match range.unwrap_or("24h") {
        "24h" => Ok(("24h", 24 * 60 * 60)),
        "7d" => Ok(("7d", 7 * 24 * 60 * 60)),
        _ => Err("range must be one of: 24h, 7d"),
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn price_item(tick: PriceTick) -> PriceItem {
    PriceItem {
        id: tick.token,
        price_usdc: tick.price_usdc,
        ts: tick.ts,
        via: tick.via,
    }
}

fn bad_request(error: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(PricesResponse {
            success: false,
            data: None,
            error: Some(error.into()),
        }),
    )
        .into_response()
}

fn history_bad_request(error: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(PriceHistoryResponse {
            success: false,
            data: None,
            error: Some(error.into()),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{config::AppConfig, price_store::PriceStore, state::AppState},
        axum::{
            extract::{Query, State},
            http::StatusCode,
        },
        dex_adapters::{rpc::SorobanRpc, token_metadata::TokenMetadataStore},
        router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
        serde_json::Value,
        std::sync::Arc,
        tempfile::tempdir,
        tokio::sync::RwLock,
    };

    fn test_state(price_store: Option<Arc<PriceStore>>) -> AppState {
        let config = AppConfig::default();
        let rpc = Arc::new(SorobanRpc::new(&config.rpc_url, &config.network_passphrase));
        AppState {
            engine: Arc::new(RwLock::new(Arc::new(QuoteEngine::new(
                PathFinderConfig::default(),
                SplitConfig::default(),
            )))),
            config,
            token_metadata: Arc::new(TokenMetadataStore::new(rpc.clone())),
            rpc,
            pool_state_store: None,
            telegram: None,
            price_store,
        }
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn missing_ids_is_400() {
        let response = get_prices(State(test_state(None)), Query(PricesQuery { ids: None })).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_history_id_is_400() {
        let response = get_price_history(
            State(test_state(None)),
            Query(PriceHistoryQuery { id: None, range: None }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn history_without_store_returns_empty_points() {
        let response = get_price_history(
            State(test_state(None)),
            Query(PriceHistoryQuery {
                id: Some("TOKEN".into()),
                range: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["points"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn prices_returns_seeded_latest_tick() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prices.db");
        std::env::set_var("PRICE_DB_PATH", &path);
        let store = Arc::new(PriceStore::open(&path).unwrap());
        store.insert_tick("TOKEN", 1_710_000_000, 0.42, "usdc").unwrap();

        let response = get_prices(
            State(test_state(Some(store))),
            Query(PricesQuery {
                ids: Some(" TOKEN ".into()),
            }),
        )
        .await;

        std::env::remove_var("PRICE_DB_PATH");
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["prices"][0]["id"], "TOKEN");
        assert_eq!(json["data"]["prices"][0]["price_usdc"], 0.42);
        assert_eq!(json["data"]["prices"][0]["ts"], 1_710_000_000_i64);
        assert_eq!(json["data"]["prices"][0]["via"], "usdc");
    }
}

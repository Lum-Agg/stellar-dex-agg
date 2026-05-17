use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use router_engine::types::{RouteRequest, TokenId};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ============================================================
// GET /api/v1/quote
// ============================================================

#[derive(Deserialize)]
pub struct QuoteQuery {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub slippage: Option<f64>,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<QuoteData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct QuoteData {
    pub expected_output: String,
    pub minimum_output: String,
    pub price_impact: f64,
    pub is_split: bool,
    pub sub_routes: Vec<SubRouteData>,
    pub compute_time_ms: u64,
}

#[derive(Serialize)]
pub struct SubRouteData {
    pub source: String,
    pub path: Vec<String>,
    pub amount_in: String,
    pub amount_out: String,
    pub percentage: f64,
}

pub async fn get_quote(
    State(state): State<AppState>,
    Query(params): Query<QuoteQuery>,
) -> impl IntoResponse {
    let amount_in: u128 = match params.amount_in.parse() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(QuoteResponse {
                    success: false,
                    data: None,
                    error: Some("Invalid amount_in".to_string()),
                }),
            );
        }
    };

    let slippage_bps = params
        .slippage
        .map(|s| (s * 100.0) as u32)
        .unwrap_or(50); // default 0.5%

    let request = RouteRequest {
        token_in: TokenId::from_str_auto(&params.token_in),
        token_out: TokenId::from_str_auto(&params.token_out),
        amount_in,
        slippage_bps: Some(slippage_bps),
        max_hops: None,
        max_splits: None,
    };

    let route = state.engine.get_route(&request).await;

    if route.sub_orders.is_empty() {
        return (
            StatusCode::OK,
            Json(QuoteResponse {
                success: false,
                data: None,
                error: Some("No route available for this pair".to_string()),
            }),
        );
    }

    let sub_routes: Vec<SubRouteData> = route
        .sub_orders
        .iter()
        .map(|so| SubRouteData {
            source: so.path.sources.join(" → "),
            path: so.path.tokens.iter().map(|t| t.canonical()).collect(),
            amount_in: so.amount_in.to_string(),
            amount_out: so.expected_amount_out.to_string(),
            percentage: so.fraction * 100.0,
        })
        .collect();

    (
        StatusCode::OK,
        Json(QuoteResponse {
            success: true,
            data: Some(QuoteData {
                expected_output: route.total_expected_out.to_string(),
                minimum_output: route.minimum_out.to_string(),
                price_impact: route.price_impact_bps as f64 / 100.0,
                is_split: route.is_split,
                sub_routes,
                compute_time_ms: route.compute_time_ms,
            }),
            error: None,
        }),
    )
}

// ============================================================
// POST /api/v1/swap
// ============================================================

#[derive(Deserialize)]
pub struct SwapRequest {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub slippage: f64,
    pub user_public_key: String,
}

#[derive(Serialize)]
pub struct SwapResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SwapData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SwapData {
    pub unsigned_tx_xdr: String,
    pub simulation: SimulationData,
    pub route: QuoteData,
}

#[derive(Serialize)]
pub struct SimulationData {
    pub success: bool,
    pub actual_output: Option<String>,
    pub fee: Option<String>,
    pub error: Option<String>,
}

pub async fn build_swap(
    State(_state): State<AppState>,
    Json(_body): Json<SwapRequest>,
) -> impl IntoResponse {
    // TODO: Implement transaction building
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(SwapResponse {
            success: false,
            data: None,
            error: Some("Transaction building not yet implemented".to_string()),
        }),
    )
}

// ============================================================
// GET /api/v1/tokens
// ============================================================

#[derive(Serialize)]
pub struct TokensResponse {
    pub tokens: Vec<TokenInfo>,
}

#[derive(Serialize)]
pub struct TokenInfo {
    pub id: String,
    pub symbol: String,
    pub name: String,
}

pub async fn list_tokens(State(state): State<AppState>) -> impl IntoResponse {
    // Return well-known tokens + tokens discovered from pool graph
    // For now, return a curated list of popular Stellar tokens with their SAC addresses
    let tokens = vec![
        TokenInfo { id: "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string(), symbol: "XLM".to_string(), name: "Stellar Lumens".to_string() },
        TokenInfo { id: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".to_string(), symbol: "USDC".to_string(), name: "USD Coin".to_string() },
        TokenInfo { id: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC".to_string(), symbol: "EURC".to_string(), name: "Euro Coin".to_string() },
        TokenInfo { id: "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV".to_string(), symbol: "AQUA".to_string(), name: "Aquarius".to_string() },
        TokenInfo { id: "CBZVSNVB55ANF3QVVZJGD6EBOCTT3BKYZXFHPBHA7DCJZ5CUNFPZRSR3".to_string(), symbol: "yXLM".to_string(), name: "Yield XLM".to_string() },
        TokenInfo { id: "CAAP2HKDLH7C2GCEGJGKYADET2MUTPBXBFGFYLU7JKDZ7IAFNWPXQ".to_string(), symbol: "BTC".to_string(), name: "Bitcoin (wrapped)".to_string() },
        TokenInfo { id: "CAZAQB3D7KSLSNOSQKYD2V4JP5V2Y3B4RDJZRLBFCCIXDCTE3WHSY3UE".to_string(), symbol: "ETH".to_string(), name: "Ethereum (wrapped)".to_string() },
        TokenInfo { id: "CCGIMRMF6MFQFGSXORCPUQPJLMCUNZYW5LXNHZGBRT3TYHKV4BALBHP3".to_string(), symbol: "FIDR".to_string(), name: "Fidr Token".to_string() },
    ];

    Json(TokensResponse { tokens })
}

// ============================================================
// GET /api/v1/health
// ============================================================

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub adapters: Vec<AdapterHealth>,
}

#[derive(Serialize)]
pub struct AdapterHealth {
    pub id: String,
    pub healthy: bool,
}

pub async fn health_check(State(_state): State<AppState>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        adapters: vec![],
    })
}

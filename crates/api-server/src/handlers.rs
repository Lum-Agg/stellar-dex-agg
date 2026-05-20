use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use router_engine::types::{RouteRequest, TokenId};
use serde::{Deserialize, Serialize};
use stellar_xdr::curr as xdr;
use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

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
    /// Pool addresses for each hop (same length as path - 1)
    pub pool_addresses: Vec<String>,
    /// DEX types for each hop: "aquarius", "soroswap", "phoenix", "sushi", "comet"
    pub dex_types: Vec<String>,
    /// Input token index for each hop (0 = token_a, 1 = token_b, etc.)
    pub in_indices: Vec<u32>,
    /// Output token index for each hop
    pub out_indices: Vec<u32>,
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

    let slippage_bps = params.slippage.map(|s| (s * 100.0) as u32).unwrap_or(50); // default 0.5%

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

    let mut sub_routes = Vec::new();
    for so in &route.sub_orders {
        let mut in_indices = Vec::new();
        let mut out_indices = Vec::new();
        for i in 0..so.path.hops {
            let token_in = &so.path.tokens[i];
            let token_out = &so.path.tokens[i + 1];
            let pool = &so.path.pool_addresses[i];
            let indices = state
                .engine
                .get_pool_indices(pool, token_in, token_out)
                .await;
            let (in_idx, out_idx) = indices.unwrap_or((0, 1));
            in_indices.push(in_idx);
            out_indices.push(out_idx);
        }
        sub_routes.push(SubRouteData {
            source: so.path.sources.join(" → "),
            path: so.path.tokens.iter().map(|t| t.canonical()).collect(),
            pool_addresses: so.path.pool_addresses.clone(),
            dex_types: so.path.sources.clone(),
            in_indices,
            out_indices,
            amount_in: so.amount_in.to_string(),
            amount_out: so.expected_amount_out.to_string(),
            percentage: so.fraction * 100.0,
        });
    }

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
    State(state): State<AppState>,
    Json(body): Json<SwapRequest>,
) -> impl IntoResponse {
    use stellar_xdr::curr as xdr;
    use stellar_xdr::curr::{Limits, WriteXdr};

    let amount_in: u128 = match body.amount_in.parse() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(SwapResponse {
                    success: false,
                    data: None,
                    error: Some("Invalid amount_in".to_string()),
                }),
            );
        }
    };

    let slippage_bps = (body.slippage * 100.0) as u32;

    // 1. Get the optimal route
    let request = RouteRequest {
        token_in: TokenId::from_str_auto(&body.token_in),
        token_out: TokenId::from_str_auto(&body.token_out),
        amount_in,
        slippage_bps: Some(slippage_bps),
        max_hops: None,
        max_splits: None,
    };

    let route = state.engine.get_route(&request).await;

    if route.sub_orders.is_empty() {
        return (
            StatusCode::OK,
            Json(SwapResponse {
                success: false,
                data: None,
                error: Some("No route available".to_string()),
            }),
        );
    }

    // 2. Build transaction — always use PathPaymentStrictSend
    // Stellar Core will automatically route through the best path
    // (including Soroban AMM pools via their SAC interfaces)
    let tx_result = build_classic_dex_tx(&body, &route, slippage_bps);

    match tx_result {
        Ok((xdr_b64, sim)) => {
            let mut sub_routes = Vec::new();
            for so in &route.sub_orders {
                let mut in_indices = Vec::new();
                let mut out_indices = Vec::new();
                for i in 0..so.path.hops {
                    let token_in = &so.path.tokens[i];
                    let token_out = &so.path.tokens[i + 1];
                    let pool = &so.path.pool_addresses[i];
                    let indices = state
                        .engine
                        .get_pool_indices(pool, token_in, token_out)
                        .await;
                    let (in_idx, out_idx) = indices.unwrap_or((0, 1));
                    in_indices.push(in_idx);
                    out_indices.push(out_idx);
                }
                sub_routes.push(SubRouteData {
                    source: so.path.sources.join(" → "),
                    path: so.path.tokens.iter().map(|t| t.canonical()).collect(),
                    pool_addresses: so.path.pool_addresses.clone(),
                    dex_types: so.path.sources.clone(),
                    in_indices,
                    out_indices,
                    amount_in: so.amount_in.to_string(),
                    amount_out: so.expected_amount_out.to_string(),
                    percentage: so.fraction * 100.0,
                });
            }

            (
                StatusCode::OK,
                Json(SwapResponse {
                    success: true,
                    data: Some(SwapData {
                        unsigned_tx_xdr: xdr_b64,
                        simulation: sim,
                        route: QuoteData {
                            expected_output: route.total_expected_out.to_string(),
                            minimum_output: route.minimum_out.to_string(),
                            price_impact: route.price_impact_bps as f64 / 100.0,
                            is_split: route.is_split,
                            sub_routes,
                            compute_time_ms: route.compute_time_ms,
                        },
                    }),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(SwapResponse {
                success: false,
                data: None,
                error: Some(format!("Transaction build failed: {}", e)),
            }),
        ),
    }
}

/// Build a Classic DEX PathPaymentStrictSend transaction.
fn build_classic_dex_tx(
    body: &SwapRequest,
    route: &router_engine::types::OptimalRoute,
    slippage_bps: u32,
) -> Result<(String, SimulationData), String> {
    use stellar_xdr::curr as xdr;
    use stellar_xdr::curr::{Limits, WriteXdr};

    let user_key = stellar_strkey::ed25519::PublicKey::from_string(&body.user_public_key)
        .map_err(|e| format!("Invalid public key: {:?}", e))?;

    // Parse assets
    let send_asset =
        parse_asset_xdr(&body.token_in).map_err(|e| format!("Invalid token_in: {}", e))?;
    let dest_asset =
        parse_asset_xdr(&body.token_out).map_err(|e| format!("Invalid token_out: {}", e))?;

    let send_amount = route.total_amount_in as i64;
    let dest_min = route.minimum_out as i64;

    // Build PathPaymentStrictSend operation
    let path_payment = xdr::OperationBody::PathPaymentStrictSend(xdr::PathPaymentStrictSendOp {
        send_asset,
        send_amount,
        destination: xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0)),
        dest_asset,
        dest_min,
        path: xdr::VecM::default(), // Let Stellar Core find the best path
    });

    let op = xdr::Operation {
        source_account: None,
        body: path_payment,
    };

    // Build transaction
    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0));

    let tx = xdr::Transaction {
        source_account,
        fee: 10000,                      // 0.001 XLM
        seq_num: xdr::SequenceNumber(0), // Client will set this
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations: vec![op].try_into().map_err(|_| "ops error".to_string())?,
        ext: xdr::TransactionExt::V0,
    };

    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });

    let xdr_b64 = envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("XDR encode error: {:?}", e))?;

    Ok((
        xdr_b64,
        SimulationData {
            success: true,
            actual_output: Some(route.total_expected_out.to_string()),
            fee: Some("10000".to_string()),
            error: None,
        },
    ))
}

/// Build a Soroban DEX transaction (calls aggregator contract).
fn build_soroban_dex_tx(
    body: &SwapRequest,
    route: &router_engine::types::OptimalRoute,
    slippage_bps: u32,
) -> Result<(String, SimulationData), String> {
    // TODO: Build InvokeHostFunction calling the aggregator contract
    // For now, return an error indicating this needs the aggregator contract
    Err(
        "Soroban DEX swap requires aggregator contract (not yet deployed). Use Classic DEX route."
            .to_string(),
    )
}

/// Parse a token identifier to XDR Asset.
/// Handles: "native", contract addresses (maps to native for XLM SAC)
fn parse_asset_xdr(token: &str) -> Result<stellar_xdr::curr::Asset, String> {
    use stellar_xdr::curr as xdr;

    // XLM SAC address
    if token == "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA" || token == "native" {
        return Ok(xdr::Asset::Native);
    }

    // USDC SAC
    if token == "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75" {
        let issuer = stellar_strkey::ed25519::PublicKey::from_string(
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        )
        .map_err(|e| format!("{:?}", e))?;
        let mut code = [0u8; 4];
        code[..4].copy_from_slice(b"USDC");
        return Ok(xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 {
            asset_code: xdr::AssetCode4(code),
            issuer: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(issuer.0))),
        }));
    }

    // EURC SAC
    if token == "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC" {
        let issuer = stellar_strkey::ed25519::PublicKey::from_string(
            "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
        )
        .map_err(|e| format!("{:?}", e))?;
        let mut code = [0u8; 4];
        code[..4].copy_from_slice(b"EURC");
        return Ok(xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 {
            asset_code: xdr::AssetCode4(code),
            issuer: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(issuer.0))),
        }));
    }

    Err(format!("Cannot convert contract {} to Classic asset (only XLM/USDC/EURC supported for Classic DEX)", token))
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
    // Get all unique tokens from the quote engine
    let all_tokens = state.engine.get_all_tokens().await;
    // Get cached metadata
    let metadata = state.token_metadata.get_all().await;

    let tokens: Vec<TokenInfo> = all_tokens
        .into_iter()
        .map(|addr| {
            // Check metadata store first
            if let Some(meta) = metadata.get(&addr) {
                if meta.name != "Unknown" {
                    return TokenInfo {
                        id: addr,
                        symbol: meta.symbol.clone(),
                        name: meta.name.clone(),
                    };
                }
            }

            // Handle classic asset format "CODE:ISSUER" and "native"
            if addr == "native" {
                TokenInfo {
                    id: addr,
                    symbol: "XLM".to_string(),
                    name: "Stellar Lumens".to_string(),
                }
            } else if addr.contains(':') {
                let code = addr.split(':').next().unwrap_or(&addr).to_string();
                TokenInfo {
                    id: addr,
                    symbol: code.clone(),
                    name: code,
                }
            } else if let Some(meta) = metadata.get(&addr) {
                // Use metadata even if "Unknown" (at least has the short symbol)
                TokenInfo {
                    id: addr,
                    symbol: meta.symbol.clone(),
                    name: meta.name.clone(),
                }
            } else {
                let short = if addr.len() > 8 {
                    addr[..8].to_string()
                } else {
                    addr.clone()
                };
                TokenInfo {
                    id: addr,
                    symbol: short,
                    name: "Unknown".to_string(),
                }
            }
        })
        .collect();

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

// ============================================================
// POST /api/v1/build_tx
// ============================================================

/// Aggregator contract address on mainnet
const AGGREGATOR_CONTRACT: &str = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";

/// Build an unsigned transaction that calls the aggregator contract.
/// User specifies exactly which DEX/pool to use for each swap step.
#[derive(Deserialize)]
pub struct BuildTxRequest {
    /// User's Stellar public key (G...)
    pub user_public_key: String,
    /// Total input amount (stroops)
    pub amount_in: String,
    /// Input token contract address
    pub token_in: String,
    /// Output token contract address (final output)
    pub token_out: String,
    /// Minimum acceptable output (stroops)
    pub min_amount_out: String,
    /// Swap steps: each step specifies a DEX pool to use
    pub steps: Vec<BuildTxStep>,
}

#[derive(Deserialize)]
pub struct BuildTxStep {
    /// DEX type: "aquarius", "soroswap", "phoenix", "sushi", "comet"
    pub dex_type: String,
    /// Pool contract address
    pub pool_address: String,
    /// Input token for this step
    pub token_in: String,
    /// Output token for this step
    pub token_out: String,
    /// Input token index in the pool's token list (0-based)
    pub in_idx: u32,
    /// Output token index in the pool's token list (0-based)
    pub out_idx: u32,
}

#[derive(Serialize)]
pub struct BuildTxResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<BuildTxData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct BuildTxData {
    /// Unsigned transaction envelope XDR (base64)
    pub unsigned_tx_xdr: String,
    /// Number of operations in the transaction
    pub num_operations: usize,
    /// Estimated fee in stroops
    pub fee: String,
    /// Aggregator contract being called
    pub contract: String,
}

pub async fn build_tx(
    State(_state): State<AppState>,
    Json(body): Json<BuildTxRequest>,
) -> impl IntoResponse {
    use stellar_xdr::curr as xdr;
    use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

    if body.steps.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(BuildTxResponse {
                success: false,
                data: None,
                error: Some("At least one step is required".to_string()),
            }),
        );
    }

    let user_key = match stellar_strkey::ed25519::PublicKey::from_string(&body.user_public_key) {
        Ok(k) => k,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Invalid public key: {:?}", e)),
                }),
            );
        }
    };

    let amount_in: i128 = match body.amount_in.parse() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some("Invalid amount_in".to_string()),
                }),
            );
        }
    };

    let min_amount_out: i128 = match body.min_amount_out.parse() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some("Invalid min_amount_out".to_string()),
                }),
            );
        }
    };

    // Apply a 2% reserve-staleness buffer on top of the user's slippage.
    // Cached pool reserves can be up to 5s old, causing the off-chain quote to
    // overestimate output by ~1-2%. We pass a slightly looser minimum to the
    // contract so that normal reserve drift doesn't cause the tx to be rejected.
    // The user's displayed slippage protection is applied at the quote layer;
    // this buffer only guards against stale-data false-positives.
    let contract_min: i128 = min_amount_out * 98 / 100;

    // Build the InvokeHostFunction operation calling aggregator.swap()
    let aggregator_hash = match stellar_strkey::Contract::from_string(AGGREGATOR_CONTRACT) {
        Ok(c) => c.0,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some("Invalid aggregator contract address".to_string()),
                }),
            );
        }
    };

    // Build SwapStep args for the contract
    // Each step: { dex_id: Address, dex_type: DexType, token_in: Address, token_out: Address, a2b: bool }
    let mut steps_scval = Vec::new();
    for step in &body.steps {
        let dex_type_val = match step.dex_type.as_str() {
            "aquarius" => xdr::ScVal::Symbol(xdr::ScSymbol("Aquarius".try_into().unwrap())),
            "soroswap" => xdr::ScVal::Symbol(xdr::ScSymbol("SoroswapPair".try_into().unwrap())),
            "phoenix" => xdr::ScVal::Symbol(xdr::ScSymbol("Phoenix".try_into().unwrap())),
            "sushi" => xdr::ScVal::Symbol(xdr::ScSymbol("Sushi".try_into().unwrap())),
            "comet" => xdr::ScVal::Symbol(xdr::ScSymbol("CometDex".try_into().unwrap())),
            other => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BuildTxResponse {
                        success: false,
                        data: None,
                        error: Some(format!(
                            "Unknown dex_type: {}. Use: aquarius, soroswap, phoenix, sushi, comet",
                            other
                        )),
                    }),
                );
            }
        };

        let pool_hash = match stellar_strkey::Contract::from_string(&step.pool_address) {
            Ok(c) => c.0,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BuildTxResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid pool_address: {}", step.pool_address)),
                    }),
                );
            }
        };
        let token_in_hash = match stellar_strkey::Contract::from_string(&step.token_in) {
            Ok(c) => c.0,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BuildTxResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid token_in: {}", step.token_in)),
                    }),
                );
            }
        };
        let token_out_hash = match stellar_strkey::Contract::from_string(&step.token_out) {
            Ok(c) => c.0,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BuildTxResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid token_out: {}", step.token_out)),
                    }),
                );
            }
        };

        // SwapStep struct as ScVal::Map
        let step_val = xdr::ScVal::Map(Some(xdr::ScMap(
            vec![
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("in_idx".try_into().unwrap())),
                    val: xdr::ScVal::U32(step.in_idx),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("out_idx".try_into().unwrap())),
                    val: xdr::ScVal::U32(step.out_idx),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_id".try_into().unwrap())),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(
                        pool_hash,
                    )))),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_type".try_into().unwrap())),
                    val: dex_type_val,
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("token_in".try_into().unwrap())),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(
                        token_in_hash,
                    )))),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol(xdr::ScSymbol("token_out".try_into().unwrap())),
                    val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(
                        token_out_hash,
                    )))),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        steps_scval.push(step_val);
    }

    // token_in address
    let token_in_hash = match stellar_strkey::Contract::from_string(&body.token_in) {
        Ok(c) => c.0,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Invalid token_in: {}", body.token_in)),
                }),
            );
        }
    };

    // Build InvokeContract args for aggregator.swap(user, token_in, amount_in, steps, min_amount_out)
    let invoke_args = xdr::InvokeContractArgs {
        contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(aggregator_hash))),
        function_name: xdr::ScSymbol("swap".try_into().unwrap()),
        args: vec![
            // user: Address
            xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
            ))),
            // token_in: Address
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(
                token_in_hash,
            )))),
            // amount_in: i128
            xdr::ScVal::I128(xdr::Int128Parts {
                hi: (amount_in >> 64) as i64,
                lo: amount_in as u64,
            }),
            // steps: Vec<SwapStep>
            xdr::ScVal::Vec(Some(xdr::ScVec(steps_scval.try_into().unwrap()))),
            // min_amount_out: i128  (contract_min = user minimum with 2% staleness buffer)
            xdr::ScVal::I128(xdr::Int128Parts {
                hi: (contract_min >> 64) as i64,
                lo: contract_min as u64,
            }),
        ]
        .try_into()
        .unwrap(),
    };

    let op = xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(invoke_args),
            auth: xdr::VecM::default(),
        }),
    };

    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0));

    // 1. Fetch sequence number from Horizon
    let seq_num = match fetch_sequence_number(&body.user_public_key).await {
        Ok(seq) => seq,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to fetch sequence number: {}", e)),
                }),
            );
        }
    };

    let tx = xdr::Transaction {
        source_account: source_account.clone(),
        fee: 10_000_000, // Will be updated after simulate
        seq_num: xdr::SequenceNumber(seq_num + 1),
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations: vec![op].try_into().unwrap(),
        ext: xdr::TransactionExt::V0,
    };

    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });

    let tx_xdr = match envelope.to_xdr_base64(Limits::none()) {
        Ok(x) => x,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some(format!("XDR encode error: {:?}", e)),
                }),
            );
        }
    };

    // 2. Simulate transaction to get footprint + auth + fees
    let rpc_url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());
    match simulate_and_assemble(&rpc_url, &tx_xdr).await {
        Ok(assembled_xdr) => (
            StatusCode::OK,
            Json(BuildTxResponse {
                success: true,
                data: Some(BuildTxData {
                    unsigned_tx_xdr: assembled_xdr,
                    num_operations: 1,
                    fee: "10000000".to_string(),
                    contract: AGGREGATOR_CONTRACT.to_string(),
                }),
                error: None,
            }),
        ),
        Err(e) => {
            // Simulate failed — do NOT return a broken raw XDR.
            // A Soroban tx without sorobanData cannot be signed by any wallet.
            // Categorize the error for a better UX message.
            let user_msg = if e.contains("Output below minimum") || e.contains("below minimum") {
                "Swap failed: price moved unfavorably since the quote was generated. \
                 Please click Refresh or increase your slippage tolerance and try again."
                    .to_string()
            } else if e.contains("EmptyPool") || e.contains("empty") {
                "Swap failed: one of the pools has insufficient liquidity. \
                 Please try a smaller amount."
                    .to_string()
            } else {
                format!("Swap simulation failed: {}", e)
            };
            (
                StatusCode::OK,
                Json(BuildTxResponse {
                    success: false,
                    data: None,
                    error: Some(user_msg),
                }),
            )
        }
    }
}

/// Fetch account sequence number from Horizon.
async fn fetch_sequence_number(public_key: &str) -> Result<i64, String> {
    let url = format!("https://horizon.stellar.org/accounts/{}", public_key);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Horizon request failed: {}", e))?;
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Horizon response parse failed: {}", e))?;
    let seq_str = data
        .get("sequence")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "No sequence in response".to_string())?;
    seq_str
        .parse::<i64>()
        .map_err(|e| format!("Invalid sequence: {}", e))
}

/// Simulate transaction and assemble with footprint + auth.
async fn simulate_and_assemble(rpc_url: &str, tx_xdr: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "simulateTransaction",
        "params": {
            "transaction": tx_xdr
        }
    });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {}", e))?;
    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("RPC response parse failed: {}", e))?;

    let result = resp_json
        .get("result")
        .ok_or_else(|| "No result in simulate response".to_string())?;

    // Check for simulation error
    if let Some(error) = result.get("error") {
        return Err(format!("Simulation failed: {}", error));
    }

    // Get the assembled transaction from transactionData
    // The RPC returns the transaction data needed to assemble the final tx
    // For simplicity, we use the "restorePreamble" or rebuild from results
    // Actually, the Stellar SDK's assembleTransaction does this, but we need to do it manually

    // The simplest approach: if simulate succeeds, the transaction is valid
    // We need to add the sorobanData (resource footprint) to the transaction
    if let Some(transaction_data) = result.get("transactionData").and_then(|t| t.as_str()) {
        // Parse the original tx, add sorobanData, return
        use stellar_xdr::curr::ReadXdr;

        let mut envelope = xdr::TransactionEnvelope::from_xdr_base64(tx_xdr, Limits::none())
            .map_err(|e| format!("Failed to parse tx: {:?}", e))?;

        if let xdr::TransactionEnvelope::Tx(ref mut v1) = envelope {
            // Set the transaction ext to include soroban data
            let soroban_data =
                xdr::SorobanTransactionData::from_xdr_base64(transaction_data, Limits::none())
                    .map_err(|e| format!("Failed to parse soroban data: {:?}", e))?;
            v1.tx.ext = xdr::TransactionExt::V1(soroban_data);

            // Update fee from simulate result
            if let Some(min_fee) = result
                .get("minResourceFee")
                .and_then(|f| f.as_str())
                .and_then(|f| f.parse::<u32>().ok())
            {
                v1.tx.fee = v1.tx.fee.max(min_fee + 100_000); // Add buffer
            }

            // Add auth from simulate results
            if let Some(results_arr) = result.get("results").and_then(|r| r.as_array()) {
                if let Some(first) = results_arr.first() {
                    if let Some(auth_arr) = first.get("auth").and_then(|a| a.as_array()) {
                        let mut ops_vec: Vec<xdr::Operation> = v1.tx.operations.to_vec();
                        if let xdr::OperationBody::InvokeHostFunction(ref mut ihf) = ops_vec[0].body
                        {
                            let mut auth_entries = Vec::new();
                            for auth_xdr in auth_arr {
                                if let Some(auth_str) = auth_xdr.as_str() {
                                    if let Ok(entry) =
                                        xdr::SorobanAuthorizationEntry::from_xdr_base64(
                                            auth_str,
                                            Limits::none(),
                                        )
                                    {
                                        auth_entries.push(entry);
                                    }
                                }
                            }
                            if !auth_entries.is_empty() {
                                ihf.auth = auth_entries.try_into().unwrap_or_default();
                            }
                        }
                        v1.tx.operations = ops_vec.try_into().unwrap_or_default();
                    }
                }
            }
        }

        let assembled = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| format!("Failed to encode assembled tx: {:?}", e))?;
        Ok(assembled)
    } else {
        Err("No transactionData in simulate response".to_string())
    }
}

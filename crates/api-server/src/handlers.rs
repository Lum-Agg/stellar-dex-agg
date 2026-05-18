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
    State(state): State<AppState>,
    Json(body): Json<SwapRequest>,
) -> impl IntoResponse {
    use stellar_xdr::curr as xdr;
    use stellar_xdr::curr::{Limits, WriteXdr};

    let amount_in: u128 = match body.amount_in.parse() {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(SwapResponse {
                success: false, data: None,
                error: Some("Invalid amount_in".to_string()),
            }));
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
        return (StatusCode::OK, Json(SwapResponse {
            success: false, data: None,
            error: Some("No route available".to_string()),
        }));
    }

    // 2. Build transaction — always use PathPaymentStrictSend
    // Stellar Core will automatically route through the best path
    // (including Soroban AMM pools via their SAC interfaces)
    let tx_result = build_classic_dex_tx(&body, &route, slippage_bps);

    match tx_result {
        Ok((xdr_b64, sim)) => {
            let sub_routes: Vec<SubRouteData> = route.sub_orders.iter().map(|so| SubRouteData {
                source: so.path.sources.join(" → "),
                path: so.path.tokens.iter().map(|t| t.canonical()).collect(),
                amount_in: so.amount_in.to_string(),
                amount_out: so.expected_amount_out.to_string(),
                percentage: so.fraction * 100.0,
            }).collect();

            (StatusCode::OK, Json(SwapResponse {
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
            }))
        }
        Err(e) => {
            (StatusCode::OK, Json(SwapResponse {
                success: false, data: None,
                error: Some(format!("Transaction build failed: {}", e)),
            }))
        }
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
    let send_asset = parse_asset_xdr(&body.token_in)
        .map_err(|e| format!("Invalid token_in: {}", e))?;
    let dest_asset = parse_asset_xdr(&body.token_out)
        .map_err(|e| format!("Invalid token_out: {}", e))?;

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
        fee: 10000, // 0.001 XLM
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

    Ok((xdr_b64, SimulationData {
        success: true,
        actual_output: Some(route.total_expected_out.to_string()),
        fee: Some("10000".to_string()),
        error: None,
    }))
}

/// Build a Soroban DEX transaction (calls aggregator contract).
fn build_soroban_dex_tx(
    body: &SwapRequest,
    route: &router_engine::types::OptimalRoute,
    slippage_bps: u32,
) -> Result<(String, SimulationData), String> {
    // TODO: Build InvokeHostFunction calling the aggregator contract
    // For now, return an error indicating this needs the aggregator contract
    Err("Soroban DEX swap requires aggregator contract (not yet deployed). Use Classic DEX route.".to_string())
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
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        ).map_err(|e| format!("{:?}", e))?;
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
            "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2"
        ).map_err(|e| format!("{:?}", e))?;
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
    /// Direction: true = token_a -> token_b (token0 -> token1)
    pub a2b: bool,
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
    use stellar_xdr::curr::{Limits, WriteXdr};

    if body.steps.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
            success: false, data: None,
            error: Some("At least one step is required".to_string()),
        }));
    }

    let user_key = match stellar_strkey::ed25519::PublicKey::from_string(&body.user_public_key) {
        Ok(k) => k,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                success: false, data: None,
                error: Some(format!("Invalid public key: {:?}", e)),
            }));
        }
    };

    let amount_in: i128 = match body.amount_in.parse() {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                success: false, data: None,
                error: Some("Invalid amount_in".to_string()),
            }));
        }
    };

    let min_amount_out: i128 = match body.min_amount_out.parse() {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                success: false, data: None,
                error: Some("Invalid min_amount_out".to_string()),
            }));
        }
    };

    // Build the InvokeHostFunction operation calling aggregator.swap()
    let aggregator_hash = match stellar_strkey::Contract::from_string(AGGREGATOR_CONTRACT) {
        Ok(c) => c.0,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(BuildTxResponse {
                success: false, data: None,
                error: Some("Invalid aggregator contract address".to_string()),
            }));
        }
    };

    // Build SwapStep args for the contract
    // Each step: { dex_id: Address, dex_type: DexType, token_in: Address, token_out: Address, a2b: bool }
    let mut steps_scval = Vec::new();
    for step in &body.steps {
        let dex_type_val = match step.dex_type.as_str() {
            "aquarius" => xdr::ScVal::Vec(Some(xdr::ScVec(vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("Aquarius".try_into().unwrap()))
            ].try_into().unwrap()))),
            "soroswap" => xdr::ScVal::Vec(Some(xdr::ScVec(vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("SoroswapPair".try_into().unwrap()))
            ].try_into().unwrap()))),
            "phoenix" => xdr::ScVal::Vec(Some(xdr::ScVec(vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("Phoenix".try_into().unwrap()))
            ].try_into().unwrap()))),
            "sushi" => xdr::ScVal::Vec(Some(xdr::ScVec(vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("Sushi".try_into().unwrap()))
            ].try_into().unwrap()))),
            "comet" => xdr::ScVal::Vec(Some(xdr::ScVec(vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("CometDex".try_into().unwrap()))
            ].try_into().unwrap()))),
            other => {
                return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                    success: false, data: None,
                    error: Some(format!("Unknown dex_type: {}. Use: aquarius, soroswap, phoenix, sushi, comet", other)),
                }));
            }
        };

        let pool_hash = match stellar_strkey::Contract::from_string(&step.pool_address) {
            Ok(c) => c.0,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                    success: false, data: None,
                    error: Some(format!("Invalid pool_address: {}", step.pool_address)),
                }));
            }
        };
        let token_in_hash = match stellar_strkey::Contract::from_string(&step.token_in) {
            Ok(c) => c.0,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                    success: false, data: None,
                    error: Some(format!("Invalid token_in: {}", step.token_in)),
                }));
            }
        };
        let token_out_hash = match stellar_strkey::Contract::from_string(&step.token_out) {
            Ok(c) => c.0,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                    success: false, data: None,
                    error: Some(format!("Invalid token_out: {}", step.token_out)),
                }));
            }
        };

        // SwapStep struct as ScVal::Map
        let step_val = xdr::ScVal::Map(Some(xdr::ScMap(vec![
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("a2b".try_into().unwrap())),
                val: xdr::ScVal::Bool(step.a2b),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_id".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_type".try_into().unwrap())),
                val: dex_type_val,
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_in".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_out".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
            },
        ].try_into().unwrap())));

        steps_scval.push(step_val);
    }

    // token_in address
    let token_in_hash = match stellar_strkey::Contract::from_string(&body.token_in) {
        Ok(c) => c.0,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(BuildTxResponse {
                success: false, data: None,
                error: Some(format!("Invalid token_in: {}", body.token_in)),
            }));
        }
    };

    // Build InvokeContract args for aggregator.swap(user, token_in, amount_in, steps, min_amount_out)
    let invoke_args = xdr::InvokeContractArgs {
        contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(aggregator_hash))),
        function_name: xdr::ScSymbol("swap".try_into().unwrap()),
        args: vec![
            // user: Address
            xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0))
            ))),
            // token_in: Address
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            // amount_in: i128
            xdr::ScVal::I128(xdr::Int128Parts {
                hi: (amount_in >> 64) as i64,
                lo: amount_in as u64,
            }),
            // steps: Vec<SwapStep>
            xdr::ScVal::Vec(Some(xdr::ScVec(steps_scval.try_into().unwrap()))),
            // min_amount_out: i128
            xdr::ScVal::I128(xdr::Int128Parts {
                hi: (min_amount_out >> 64) as i64,
                lo: min_amount_out as u64,
            }),
        ].try_into().unwrap(),
    };

    let op = xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(invoke_args),
            auth: xdr::VecM::default(),
        }),
    };

    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0));
    let fee = 10_000_000u32; // 1 XLM (Soroban txs need higher fees)

    let tx = xdr::Transaction {
        source_account,
        fee,
        seq_num: xdr::SequenceNumber(0), // Client must set this
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations: vec![op].try_into().unwrap(),
        ext: xdr::TransactionExt::V0,
    };

    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });

    match envelope.to_xdr_base64(Limits::none()) {
        Ok(xdr_b64) => {
            (StatusCode::OK, Json(BuildTxResponse {
                success: true,
                data: Some(BuildTxData {
                    unsigned_tx_xdr: xdr_b64,
                    num_operations: 1,
                    fee: fee.to_string(),
                    contract: AGGREGATOR_CONTRACT.to_string(),
                }),
                error: None,
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(BuildTxResponse {
                success: false, data: None,
                error: Some(format!("XDR encode error: {:?}", e)),
            }))
        }
    }
}

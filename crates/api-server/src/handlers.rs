use {
    crate::{soroban_prepare::prepare_transaction_xdr, state::AppState},
    axum::{
        extract::{Query, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    },
    router_engine::{
        types::{RouteRequest, TokenId},
        QuoteEngine,
    },
    serde::{Deserialize, Serialize},
    std::sync::Arc,
    stellar_xdr::{
        curr as xdr,
        curr::{Limits, WriteXdr},
    },
};

/// GET / — human-friendly landing when reviewers open the API host in a
/// browser.
pub async fn api_root() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "LumAgg API",
        "status": "ok",
        "endpoints": {
            "health": "/api/v1/health",
            "quote": "/api/v1/quote",
            "build_tx": "/api/v1/build_tx",
            "tokens": "/api/v1/tokens"
        },
        "repository": "https://github.com/ligulfzhou/stellar-dex-agg"
    }))
}

// ============================================================
// GET /api/v1/quote
// ============================================================

#[derive(Deserialize)]
pub struct QuoteQuery {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub slippage: Option<f64>,
    pub debug: Option<u8>,
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
    /// Total input stroops used for this quote (sum of sub-route amounts)
    pub amount_in: String,
    pub expected_output: String,
    pub minimum_output: String,
    pub price_impact: f64,
    pub is_split: bool,
    pub sub_routes: Vec<SubRouteData>,
    pub compute_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<QuoteDebugData>,
}

#[derive(Serialize)]
pub struct QuoteDebugData {
    pub quoted_paths_count: usize,
    pub candidate_paths_count: usize,
    pub best_single_out: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_best_out: Option<String>,
    pub best_single_impact_bps: u32,
    pub split_threshold_bps: u32,
    pub competitive_delta_bps: u32,
    pub min_split_fraction_bps: u32,
    pub split_attempted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_rejected_reason: Option<String>,
    pub optimization_strategy: String,
    pub used_rest_best_approximation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_total_out: Option<String>,
    pub dust_filtered_legs: usize,
    pub candidate_routes: Vec<QuoteDebugCandidateData>,
    pub planned_split: Vec<QuoteDebugPlannedSplitData>,
    pub improvement_bps: u32,
}

#[derive(Serialize)]
pub struct QuoteDebugCandidateData {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub amount_out: String,
    pub price_impact_bps: u32,
}

#[derive(Serialize)]
pub struct QuoteDebugPlannedSplitData {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub amount_in: String,
    pub expected_amount_out: String,
    pub fraction_bps: u32,
}

#[derive(Serialize)]
pub struct SubRouteData {
    pub source: String,
    pub path: Vec<String>,
    /// Pool addresses for each hop (same length as path - 1)
    pub pool_addresses: Vec<String>,
    /// DEX types for each hop: "aquarius", "soroswap", "phoenix", "sushi",
    /// "comet", "classic_dex"
    pub dex_types: Vec<String>,
    /// Input token index for each hop (0 = token_a, 1 = token_b, etc.)
    pub in_indices: Vec<u32>,
    /// Output token index for each hop
    pub out_indices: Vec<u32>,
    pub amount_in: String,
    pub amount_out: String,
    pub percentage: f64,
}

pub async fn get_quote(State(state): State<AppState>, Query(params): Query<QuoteQuery>) -> impl IntoResponse {
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
    let include_debug = params.debug.unwrap_or(0) != 0;

    let request = RouteRequest {
        token_in: TokenId::from_str_auto(&params.token_in),
        token_out: TokenId::from_str_auto(&params.token_out),
        amount_in,
        slippage_bps: Some(slippage_bps),
        max_hops: None,
        max_splits: None,
    };

    let engine = state.current_engine().await;
    let route = state.quote_route(&request).await;

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
            let (in_idx, out_idx) = match engine.get_pool_indices(pool, token_in, token_out).await {
                Some(indices) => indices,
                None => {
                    return (
                        StatusCode::OK,
                        Json(QuoteResponse {
                            success: false,
                            data: None,
                            error: Some(format!(
                                "Cannot resolve pool token indices for {} → {} on {}",
                                token_in.canonical(),
                                token_out.canonical(),
                                pool
                            )),
                        }),
                    );
                }
            };
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
                amount_in: route.total_amount_in.to_string(),
                expected_output: route.total_expected_out.to_string(),
                minimum_output: route.minimum_out.to_string(),
                price_impact: route.price_impact_bps as f64 / 100.0,
                is_split: route.is_split,
                sub_routes,
                compute_time_ms: route.compute_time_ms,
                debug: if include_debug {
                    route.debug.as_ref().map(|d| QuoteDebugData {
                        quoted_paths_count: d.quoted_paths_count,
                        candidate_paths_count: d.candidate_paths_count,
                        best_single_out: d.best_single_out.to_string(),
                        second_best_out: d.second_best_out.map(|v| v.to_string()),
                        best_single_impact_bps: d.best_single_impact_bps,
                        split_threshold_bps: d.split_threshold_bps,
                        competitive_delta_bps: d.competitive_delta_bps,
                        min_split_fraction_bps: d.min_split_fraction_bps,
                        split_attempted: d.split_attempted,
                        split_rejected_reason: d.split_rejected_reason.clone(),
                        optimization_strategy: d.optimization_strategy.clone(),
                        used_rest_best_approximation: d.used_rest_best_approximation,
                        split_total_out: d.split_total_out.map(|v| v.to_string()),
                        dust_filtered_legs: d.dust_filtered_legs,
                        candidate_routes: d
                            .candidate_routes
                            .iter()
                            .map(|route| QuoteDebugCandidateData {
                                source: route.source.clone(),
                                path: route.path.clone(),
                                pool_addresses: route.pool_addresses.clone(),
                                amount_out: route.amount_out.to_string(),
                                price_impact_bps: route.price_impact_bps,
                            })
                            .collect(),
                        planned_split: d
                            .planned_split
                            .iter()
                            .map(|leg| QuoteDebugPlannedSplitData {
                                source: leg.source.clone(),
                                path: leg.path.clone(),
                                pool_addresses: leg.pool_addresses.clone(),
                                amount_in: leg.amount_in.to_string(),
                                expected_amount_out: leg.expected_amount_out.to_string(),
                                fraction_bps: leg.fraction_bps,
                            })
                            .collect(),
                        improvement_bps: route.improvement_bps,
                    })
                } else {
                    None
                },
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

async fn route_to_sub_routes(
    engine: &Arc<QuoteEngine>,
    route: &router_engine::types::OptimalRoute,
) -> Result<(Vec<SubRouteData>, Vec<BuildTxSubRoute>), String> {
    let mut sub_routes = Vec::new();
    let mut build_sub_routes = Vec::new();

    for so in &route.sub_orders {
        let mut in_indices = Vec::new();
        let mut out_indices = Vec::new();
        let mut build_steps = Vec::new();

        for i in 0..so.path.hops {
            let token_in = &so.path.tokens[i];
            let token_out = &so.path.tokens[i + 1];
            let pool = &so.path.pool_addresses[i];
            let dex_type = so.path.sources[i].clone();

            let (in_idx, out_idx) = engine
                .get_pool_indices(pool, token_in, token_out)
                .await
                .ok_or_else(|| {
                    format!(
                        "Cannot resolve pool token indices for {} → {} on {}",
                        token_in.canonical(),
                        token_out.canonical(),
                        pool
                    )
                })?;

            in_indices.push(in_idx);
            out_indices.push(out_idx);
            build_steps.push(BuildTxStep {
                dex_type,
                pool_address: pool.clone(),
                token_in: token_in.canonical(),
                token_out: token_out.canonical(),
                in_idx,
                out_idx,
            });
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
        build_sub_routes.push(BuildTxSubRoute {
            amount_in: so.amount_in.to_string(),
            steps: build_steps,
        });
    }

    Ok((sub_routes, build_sub_routes))
}

pub async fn build_swap(State(state): State<AppState>, Json(body): Json<SwapRequest>) -> impl IntoResponse {
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

    let engine = state.current_engine().await;
    let route = state.quote_route(&request).await;

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

    let (sub_routes, build_sub_routes) = match route_to_sub_routes(&engine, &route).await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(SwapResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                }),
            );
        }
    };

    let build_req = BuildTxRequest {
        user_public_key: body.user_public_key.clone(),
        amount_in: route.total_amount_in.to_string(),
        token_in: body.token_in.clone(),
        token_out: body.token_out.clone(),
        min_amount_out: route.minimum_out.to_string(),
        sub_routes: build_sub_routes,
    };

    match build_tx_impl(&build_req).await {
        Ok(tx) => (
            StatusCode::OK,
            Json(SwapResponse {
                success: true,
                data: Some(SwapData {
                    unsigned_tx_xdr: tx.unsigned_tx_xdr,
                    simulation: SimulationData {
                        success: true,
                        actual_output: Some(route.total_expected_out.to_string()),
                        fee: Some(tx.fee),
                        error: None,
                    },
                    route: QuoteData {
                        amount_in: route.total_amount_in.to_string(),
                        expected_output: route.total_expected_out.to_string(),
                        minimum_output: route.minimum_out.to_string(),
                        price_impact: route.price_impact_bps as f64 / 100.0,
                        is_split: route.is_split,
                        sub_routes,
                        compute_time_ms: route.compute_time_ms,
                        debug: None,
                    },
                }),
                error: None,
            }),
        ),
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

pub async fn build_tx_impl(body: &BuildTxRequest) -> Result<BuildTxData, String> {
    use stellar_xdr::{
        curr as xdr,
        curr::{Limits, WriteXdr},
    };

    let user_key = stellar_strkey::ed25519::PublicKey::from_string(&body.user_public_key)
        .map_err(|e| format!("Invalid public key: {:?}", e))?;
    let amount_in: i128 = body.amount_in.parse().map_err(|_| "Invalid amount_in".to_string())?;
    let min_amount_out: i128 = body
        .min_amount_out
        .parse()
        .map_err(|_| "Invalid min_amount_out".to_string())?;

    let mut sub_routes_total: i128 = 0;
    let mut classic_subs: Vec<&BuildTxSubRoute> = Vec::new();
    let mut soroban_subs: Vec<&BuildTxSubRoute> = Vec::new();

    for sub in &body.sub_routes {
        let leg_amount: i128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        sub_routes_total += leg_amount;

        if sub_route_is_classic(sub) {
            classic_subs.push(sub);
        } else if sub_route_is_soroban(sub) {
            soroban_subs.push(sub);
        } else {
            return Err(
                "Each sub-route must be all classic_dex or all Soroban hops (no mixing within one leg)".to_string(),
            );
        }
    }

    if sub_routes_total != amount_in {
        return Err(format!(
            "sub_routes amount_in sum ({}) does not match amount_in ({})",
            sub_routes_total, amount_in
        ));
    }

    let execution = if !classic_subs.is_empty() && !soroban_subs.is_empty() {
        "hybrid"
    } else if !classic_subs.is_empty() {
        "classic"
    } else {
        "soroban"
    };

    if execution == "hybrid" {
        return Err(
            "Hybrid classic_dex + Soroban transactions are not supported on Stellar: \
             Soroban simulation rejects transactions with more than one operation. \
             Please use an all-Soroban route or an all-classic route."
                .to_string(),
        );
    }

    let contract_label = if soroban_subs.is_empty() {
        DEX_CLASSIC.to_string()
    } else {
        AGGREGATOR_CONTRACT.to_string()
    };

    let mut ops: Vec<xdr::Operation> = Vec::new();
    for sub in &classic_subs {
        let leg_amount: i128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        let dest_min = classic_dest_min_for_sub(leg_amount, amount_in, min_amount_out)?;
        ops.push(build_path_payment_op(sub, &user_key, dest_min)?);
    }

    if !soroban_subs.is_empty() {
        let soroban_subs_owned: Vec<BuildTxSubRoute> = soroban_subs.iter().map(|s| (*s).clone()).collect();
        ops.push(build_aggregator_invoke_op(
            body,
            &user_key,
            &soroban_subs_owned,
            min_amount_out,
        )?);
    }

    let num_ops = ops.len();
    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0));
    let seq_num = fetch_sequence_number(&body.user_public_key).await?;
    let base_fee = 100_000u32.saturating_mul(num_ops as u32);
    let operations = ops
        .clone()
        .try_into()
        .map_err(|_| "Too many operations in one transaction".to_string())?;

    let tx = xdr::Transaction {
        source_account: source_account.clone(),
        fee: base_fee.max(10_000),
        seq_num: xdr::SequenceNumber(seq_num + 1),
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations,
        ext: xdr::TransactionExt::V0,
    };

    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });

    let fee = base_fee.max(10_000);
    let rpc_url =
        std::env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());

    let unsigned_tx_xdr = match execution {
        "classic" => envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| format!("XDR encode error: {:?}", e))?,
        "soroban" => {
            let ops_vec: Vec<xdr::Operation> = ops.to_vec();
            prepare_transaction_xdr(&rpc_url, &body.user_public_key, seq_num as u64, &ops_vec, fee).await?
        }
        "hybrid" => {
            let full_xdr = envelope
                .to_xdr_base64(Limits::none())
                .map_err(|e| format!("XDR encode error: {:?}", e))?;
            let invoke_ops: Vec<xdr::Operation> = ops
                .iter()
                .filter(|op| matches!(op.body, xdr::OperationBody::InvokeHostFunction(_)))
                .cloned()
                .collect();
            if invoke_ops.len() != 1 {
                return Err("Hybrid tx must contain exactly one Soroban invoke".to_string());
            }
            let prepared_invoke =
                prepare_transaction_xdr(&rpc_url, &body.user_public_key, seq_num as u64, &invoke_ops, fee).await?;
            merge_prepared_invoke_into_tx(&full_xdr, &prepared_invoke)?
        }
        other => return Err(format!("Unknown execution mode: {}", other)),
    };

    Ok(BuildTxData {
        unsigned_tx_xdr,
        num_operations: num_ops,
        fee: fee.to_string(),
        contract: contract_label,
        execution: execution.to_string(),
    })
}

/// Copy sorobanData + auth from a prepared single-invoke tx into a hybrid
/// envelope.
fn merge_prepared_invoke_into_tx(full_tx_xdr: &str, prepared_invoke_xdr: &str) -> Result<String, String> {
    use stellar_xdr::curr::ReadXdr;

    let mut full = xdr::TransactionEnvelope::from_xdr_base64(full_tx_xdr, Limits::none())
        .map_err(|e| format!("parse full tx: {:?}", e))?;
    let prepared = xdr::TransactionEnvelope::from_xdr_base64(prepared_invoke_xdr, Limits::none())
        .map_err(|e| format!("parse prepared invoke tx: {:?}", e))?;

    let xdr::TransactionEnvelope::Tx(ref mut full_v1) = full else {
        return Err("Unsupported full tx envelope".to_string());
    };
    let xdr::TransactionEnvelope::Tx(prepared_v1) = prepared else {
        return Err("Unsupported prepared tx envelope".to_string());
    };

    if let xdr::TransactionExt::V1(ref soroban_data) = prepared_v1.tx.ext {
        full_v1.tx.ext = xdr::TransactionExt::V1(soroban_data.clone());
    }
    full_v1.tx.fee = full_v1.tx.fee.max(prepared_v1.tx.fee);

    if let Some(prepared_op) = prepared_v1.tx.operations.first() {
        if let xdr::OperationBody::InvokeHostFunction(ref prepared_ihf) = prepared_op.body {
            for op in full_v1.tx.operations.iter_mut() {
                if let xdr::OperationBody::InvokeHostFunction(ref mut ihf) = op.body {
                    ihf.auth = prepared_ihf.auth.clone();
                }
            }
        }
    }

    full.to_xdr_base64(Limits::none())
        .map_err(|e| format!("encode merged tx: {:?}", e))
}

/// Build unsigned envelope XDR only (no RPC simulate). Used by tests and
/// debugging.
pub async fn build_unsigned_tx_xdr(body: &BuildTxRequest) -> Result<String, String> {
    use stellar_xdr::curr::{Limits, WriteXdr};

    let user_key = stellar_strkey::ed25519::PublicKey::from_string(&body.user_public_key)
        .map_err(|e| format!("Invalid public key: {:?}", e))?;
    let amount_in: i128 = body.amount_in.parse().map_err(|_| "Invalid amount_in".to_string())?;
    let min_amount_out: i128 = body
        .min_amount_out
        .parse()
        .map_err(|_| "Invalid min_amount_out".to_string())?;

    let mut sub_routes_total: i128 = 0;
    let mut classic_subs: Vec<&BuildTxSubRoute> = Vec::new();
    let mut soroban_subs: Vec<&BuildTxSubRoute> = Vec::new();

    for sub in &body.sub_routes {
        let leg_amount: i128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        sub_routes_total += leg_amount;

        if sub_route_is_classic(sub) {
            classic_subs.push(sub);
        } else if sub_route_is_soroban(sub) {
            soroban_subs.push(sub);
        } else {
            return Err(
                "Each sub-route must be all classic_dex or all Soroban hops (no mixing within one leg)".to_string(),
            );
        }
    }

    if sub_routes_total != amount_in {
        return Err(format!(
            "sub_routes amount_in sum ({}) does not match amount_in ({})",
            sub_routes_total, amount_in
        ));
    }

    let execution = if !classic_subs.is_empty() && !soroban_subs.is_empty() {
        "hybrid"
    } else if !classic_subs.is_empty() {
        "classic"
    } else {
        "soroban"
    };

    if execution == "hybrid" {
        return Err(
            "Hybrid classic_dex + Soroban transactions are not supported on Stellar: \
             Soroban simulation rejects transactions with more than one operation. \
             Please use an all-Soroban route or an all-classic route."
                .to_string(),
        );
    }

    let mut ops: Vec<xdr::Operation> = Vec::new();
    for sub in &classic_subs {
        let leg_amount: i128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        let dest_min = classic_dest_min_for_sub(leg_amount, amount_in, min_amount_out)?;
        ops.push(build_path_payment_op(sub, &user_key, dest_min)?);
    }

    if !soroban_subs.is_empty() {
        let soroban_subs_owned: Vec<BuildTxSubRoute> = soroban_subs.iter().map(|s| (*s).clone()).collect();
        ops.push(build_aggregator_invoke_op(
            body,
            &user_key,
            &soroban_subs_owned,
            min_amount_out,
        )?);
    }

    let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0));
    let seq_num = fetch_sequence_number(&body.user_public_key).await?;
    let base_fee = 100_000u32.saturating_mul(ops.len() as u32);
    let operations = ops
        .try_into()
        .map_err(|_| "Too many operations in one transaction".to_string())?;

    let tx = xdr::Transaction {
        source_account,
        fee: base_fee.max(10_000),
        seq_num: xdr::SequenceNumber(seq_num + 1),
        cond: xdr::Preconditions::None,
        memo: xdr::Memo::None,
        operations,
        ext: xdr::TransactionExt::V0,
    };

    let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx,
        signatures: xdr::VecM::default(),
    });

    envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("XDR encode error: {:?}", e))
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
    /// Stellar.expert asset icon URL when known; empty string otherwise.
    pub logo: String,
}

fn logo_url_for_asset_id(asset_id: &str) -> Option<String> {
    if asset_id == "native" {
        return Some("https://stellar.expert/explorer/public/asset/native/icon".to_string());
    }
    if let Some((code, issuer)) = asset_id.split_once(':') {
        if !code.is_empty() && !issuer.is_empty() {
            return Some(format!(
                "https://stellar.expert/explorer/public/asset/{}-{}-1/icon",
                code, issuer
            ));
        }
    }
    None
}

/// Mainnet SAC / classic mapping for tokens returned as contract ids (C...).
const WELL_KNOWN_CONTRACT_LOGOS: &[(&str, &str)] = &[
    (
        "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        "https://stellar.expert/explorer/public/asset/native/icon",
    ),
    (
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "https://stellar.expert/explorer/public/asset/USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN-1/icon",
    ),
    (
        "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        "https://stellar.expert/explorer/public/asset/EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2-1/icon",
    ),
    (
        "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
        "https://stellar.expert/explorer/public/asset/AQUA-GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA-1/icon",
    ),
];

fn logo_for_contract(contract: &str) -> Option<String> {
    WELL_KNOWN_CONTRACT_LOGOS
        .iter()
        .find(|(c, _)| *c == contract)
        .map(|(_, url)| url.to_string())
}

fn resolve_token_logo(id: &str, name: &str, metadata_logo: Option<String>) -> String {
    metadata_logo
        .or_else(|| logo_url_for_asset_id(id))
        .or_else(|| logo_url_for_asset_id(name))
        .or_else(|| logo_for_contract(id))
        .unwrap_or_default()
}

pub async fn list_tokens(State(state): State<AppState>) -> impl IntoResponse {
    // Get all unique tokens from the quote engine
    let engine = state.current_engine().await;
    let all_tokens = engine.get_all_tokens().await;
    // Get cached metadata
    let metadata = state.token_metadata.get_all().await;

    let tokens: Vec<TokenInfo> = all_tokens
        .into_iter()
        .map(|addr| {
            // Check metadata store first
            if let Some(meta) = metadata.get(&addr) {
                if meta.name != "Unknown" {
                    let logo = resolve_token_logo(&addr, &meta.name, meta.logo.clone());
                    return TokenInfo {
                        id: addr,
                        symbol: meta.symbol.clone(),
                        name: meta.name.clone(),
                        logo,
                    };
                }
            }

            // Handle classic asset format "CODE:ISSUER" and "native"
            if addr == "native" {
                TokenInfo {
                    id: addr,
                    symbol: "XLM".to_string(),
                    name: "Stellar Lumens".to_string(),
                    logo: resolve_token_logo("native", "native", None),
                }
            } else if addr.contains(':') {
                let code = addr.split(':').next().unwrap_or(&addr).to_string();
                let logo = resolve_token_logo(&addr, &addr, None);
                TokenInfo {
                    id: addr,
                    symbol: code.clone(),
                    name: code,
                    logo,
                }
            } else if let Some(meta) = metadata.get(&addr) {
                // Use metadata even if "Unknown" (at least has the short symbol)
                let logo = resolve_token_logo(&addr, &meta.name, meta.logo.clone());
                TokenInfo {
                    id: addr,
                    symbol: meta.symbol.clone(),
                    name: meta.name.clone(),
                    logo,
                }
            } else {
                let short = if addr.len() > 8 {
                    addr[..8].to_string()
                } else {
                    addr.clone()
                };
                let logo = resolve_token_logo(&addr, "", None);
                TokenInfo {
                    id: addr,
                    symbol: short,
                    name: "Unknown".to_string(),
                    logo,
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

/// Build an unsigned transaction for the quoted route.
/// Soroban legs use aggregator `swap`; Classic legs use
/// `PathPaymentStrictSend`. Mixed routes emit both operation types in one
/// atomic transaction.
#[derive(Deserialize)]
pub struct BuildTxRequest {
    /// User's Stellar public key (G...)
    pub user_public_key: String,
    /// Total input amount (stroops); must equal sum of sub-route amounts
    pub amount_in: String,
    /// Input token contract address
    pub token_in: String,
    /// Output token contract address (final output)
    pub token_out: String,
    /// Minimum acceptable output (stroops)
    pub min_amount_out: String,
    /// Execution legs from the quote (single path = one entry)
    pub sub_routes: Vec<BuildTxSubRoute>,
}

#[derive(Clone, Deserialize)]
pub struct BuildTxSubRoute {
    /// Input allocated to this leg (stroops)
    pub amount_in: String,
    pub steps: Vec<BuildTxStep>,
}

#[derive(Clone, Deserialize)]
pub struct BuildTxStep {
    /// DEX type: "aquarius", "soroswap", "phoenix", "sushi", "comet",
    /// "classic_dex"
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
    /// Primary contract (aggregator) or "classic_dex" for PathPayment-only txs
    pub contract: String,
    /// "soroban" | "classic" | "hybrid"
    pub execution: String,
}

fn build_tx_step_scval(step: &BuildTxStep) -> Result<stellar_xdr::curr::ScVal, String> {
    use stellar_xdr::curr as xdr;

    let dex_unit_enum = |name: &str| {
        xdr::ScVal::Vec(Some(xdr::ScVec(
            vec![xdr::ScVal::Symbol(xdr::ScSymbol(name.try_into().unwrap()))]
                .try_into()
                .unwrap(),
        )))
    };

    let dex_type_val = match step.dex_type.as_str() {
        "aquarius" | "aquarius_clmm" => dex_unit_enum("Aquarius"),
        "soroswap" => dex_unit_enum("SoroswapPair"),
        "phoenix" => dex_unit_enum("Phoenix"),
        "sushi" => dex_unit_enum("Sushi"),
        "comet" => dex_unit_enum("CometDex"),
        "classic_dex" => {
            return Err("classic_dex steps must use PathPaymentStrictSend, not aggregator.swap".to_string());
        }
        other => return Err(format!("Unknown dex_type: {}", other)),
    };

    let pool_hash = stellar_strkey::Contract::from_string(&step.pool_address)
        .map_err(|_| format!("Invalid pool_address: {}", step.pool_address))?
        .0;
    let token_in_hash = stellar_strkey::Contract::from_string(&step.token_in)
        .map_err(|_| format!("Invalid token_in: {}", step.token_in))?
        .0;
    let token_out_hash = stellar_strkey::Contract::from_string(&step.token_out)
        .map_err(|_| format!("Invalid token_out: {}", step.token_out))?
        .0;

    Ok(xdr::ScVal::Map(Some(xdr::ScMap(
        vec![
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_id".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(pool_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("dex_type".try_into().unwrap())),
                val: dex_type_val,
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("in_idx".try_into().unwrap())),
                val: xdr::ScVal::U32(step.in_idx),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("out_idx".try_into().unwrap())),
                val: xdr::ScVal::U32(step.out_idx),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_in".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("token_out".try_into().unwrap())),
                val: xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
            },
        ]
        .try_into()
        .unwrap(),
    ))))
}

fn build_tx_sub_route_scval(sub: &BuildTxSubRoute) -> Result<stellar_xdr::curr::ScVal, String> {
    use stellar_xdr::curr as xdr;

    if sub.steps.is_empty() {
        return Err("Each sub-route must have at least one step".to_string());
    }
    let leg_amount: i128 = sub
        .amount_in
        .parse()
        .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;

    let mut steps_scval = Vec::new();
    for step in &sub.steps {
        steps_scval.push(build_tx_step_scval(step)?);
    }

    Ok(xdr::ScVal::Map(Some(xdr::ScMap(
        vec![
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("amount_in".try_into().unwrap())),
                val: xdr::ScVal::I128(xdr::Int128Parts {
                    hi: (leg_amount >> 64) as i64,
                    lo: leg_amount as u64,
                }),
            },
            xdr::ScMapEntry {
                key: xdr::ScVal::Symbol(xdr::ScSymbol("steps".try_into().unwrap())),
                val: xdr::ScVal::Vec(Some(xdr::ScVec(steps_scval.try_into().unwrap()))),
            },
        ]
        .try_into()
        .unwrap(),
    ))))
}

const DEX_CLASSIC: &str = "classic_dex";

fn sub_route_is_classic(sub: &BuildTxSubRoute) -> bool {
    !sub.steps.is_empty() && sub.steps.iter().all(|s| s.dex_type == DEX_CLASSIC)
}

fn sub_route_is_soroban(sub: &BuildTxSubRoute) -> bool {
    !sub.steps.is_empty() && sub.steps.iter().all(|s| s.dex_type != DEX_CLASSIC)
}

/// Per-leg minimum output for Classic `PathPaymentStrictSend`.
/// Stellar core rejects the op when `dest_min <= 0`
/// (`PATH_PAYMENT_STRICT_SEND_MALFORMED`).
fn classic_dest_min_for_sub(leg_amount_in: i128, total_amount_in: i128, min_amount_out: i128) -> Result<i64, String> {
    if min_amount_out <= 0 {
        return Err("min_amount_out must be > 0 for Classic DEX swaps (Stellar requires dest_min > 0)".to_string());
    }
    if leg_amount_in <= 0 || total_amount_in <= 0 {
        return Err("amount_in must be positive".to_string());
    }
    let dest = min_amount_out
        .saturating_mul(leg_amount_in)
        .checked_div(total_amount_in)
        .unwrap_or(0);
    let dest = dest.max(1);
    dest.try_into()
        .map_err(|_| format!("dest_min {} exceeds i64::MAX", dest))
}

fn build_path_payment_op(
    sub: &BuildTxSubRoute,
    user_key: &stellar_strkey::ed25519::PublicKey,
    dest_min: i64,
) -> Result<xdr::Operation, String> {
    let first = sub
        .steps
        .first()
        .ok_or_else(|| "classic sub-route has no steps".to_string())?;
    let last = sub
        .steps
        .last()
        .ok_or_else(|| "classic sub-route has no steps".to_string())?;
    let send_asset = parse_asset_xdr(&first.token_in).map_err(|e| format!("Invalid token_in: {}", e))?;
    let dest_asset = parse_asset_xdr(&last.token_out).map_err(|e| format!("Invalid token_out: {}", e))?;
    let send_amount = sub
        .amount_in
        .parse::<i64>()
        .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
    let path_assets: Vec<xdr::Asset> = sub
        .steps
        .iter()
        .take(sub.steps.len().saturating_sub(1))
        .map(|step| parse_asset_xdr(&step.token_out))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Invalid classic path asset: {}", e))?;

    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::PathPaymentStrictSend(xdr::PathPaymentStrictSendOp {
            send_asset,
            send_amount,
            destination: xdr::MuxedAccount::Ed25519(xdr::Uint256(user_key.0)),
            dest_asset,
            dest_min,
            path: path_assets
                .try_into()
                .map_err(|_| "classic path too long".to_string())?,
        }),
    })
}

/// Parse a token identifier to XDR Asset.
/// Handles: "native", contract addresses (maps to native for XLM SAC)
fn parse_asset_xdr(token: &str) -> Result<stellar_xdr::curr::Asset, String> {
    use stellar_xdr::curr as xdr;

    if token == "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA" || token == "native" {
        return Ok(xdr::Asset::Native);
    }

    if token == "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75" {
        let issuer =
            stellar_strkey::ed25519::PublicKey::from_string("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
                .map_err(|e| format!("{:?}", e))?;
        let mut code = [0u8; 4];
        code[..4].copy_from_slice(b"USDC");
        return Ok(xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 {
            asset_code: xdr::AssetCode4(code),
            issuer: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(issuer.0))),
        }));
    }

    if token == "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC" {
        let issuer =
            stellar_strkey::ed25519::PublicKey::from_string("GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2")
                .map_err(|e| format!("{:?}", e))?;
        let mut code = [0u8; 4];
        code[..4].copy_from_slice(b"EURC");
        return Ok(xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 {
            asset_code: xdr::AssetCode4(code),
            issuer: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(issuer.0))),
        }));
    }

    Err(format!(
        "Cannot convert contract {} to Classic asset (only XLM/USDC/EURC supported for Classic DEX)",
        token
    ))
}

fn build_aggregator_invoke_op(
    body: &BuildTxRequest,
    user_key: &stellar_strkey::ed25519::PublicKey,
    soroban_subs: &[BuildTxSubRoute],
    contract_min: i128,
) -> Result<xdr::Operation, String> {
    let aggregator_hash = stellar_strkey::Contract::from_string(AGGREGATOR_CONTRACT)
        .map_err(|_| "Invalid aggregator contract address".to_string())?
        .0;
    let token_in_hash = stellar_strkey::Contract::from_string(&body.token_in)
        .map_err(|_| format!("Invalid token_in: {}", body.token_in))?
        .0;
    let token_out_hash = stellar_strkey::Contract::from_string(&body.token_out)
        .map_err(|_| format!("Invalid token_out: {}", body.token_out))?
        .0;

    let mut sub_routes_scval = Vec::new();
    for sub in soroban_subs {
        sub_routes_scval.push(build_tx_sub_route_scval(sub)?);
    }

    let invoke_args = xdr::InvokeContractArgs {
        contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(aggregator_hash))),
        function_name: xdr::ScSymbol("swap".try_into().unwrap()),
        args: vec![
            xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
            ))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_in_hash)))),
            xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(token_out_hash)))),
            xdr::ScVal::Vec(Some(xdr::ScVec(
                sub_routes_scval
                    .try_into()
                    .map_err(|_| "too many soroban sub-routes".to_string())?,
            ))),
            xdr::ScVal::I128(xdr::Int128Parts {
                hi: (contract_min >> 64) as i64,
                lo: contract_min as u64,
            }),
        ]
        .try_into()
        .map_err(|_| "aggregator swap args error".to_string())?,
    };

    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(invoke_args),
            auth: xdr::VecM::default(),
        }),
    })
}

pub async fn build_tx(State(_state): State<AppState>, Json(body): Json<BuildTxRequest>) -> impl IntoResponse {
    if body.sub_routes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(BuildTxResponse {
                success: false,
                data: None,
                error: Some("At least one sub-route is required".to_string()),
            }),
        );
    }

    match build_tx_impl(&body).await {
        Ok(data) => (
            StatusCode::OK,
            Json(BuildTxResponse {
                success: true,
                data: Some(data),
                error: None,
            }),
        ),
        Err(e) => {
            // Simulate failed — do NOT return a broken raw XDR.
            // A Soroban tx without sorobanData cannot be signed by any wallet.
            // Categorize the error for a better UX message.
            let user_msg = if e.contains("Output below minimum") ||
                e.contains("below minimum") ||
                (e.contains("UnreachableCodeReached") && e.contains("swap"))
            {
                "Swap failed: on-chain output was below your minimum (quote vs execution drift, \
                 common on split routes). Refresh the quote, increase slippage, or retry with a single-path route."
                    .to_string()
            } else if e.contains("EmptyPool") || e.contains("empty") {
                "Swap failed: one of the pools has insufficient liquidity. \
                 Please try a smaller amount."
                    .to_string()
            } else if e.contains("resulting balance is not within the allowed range") ||
                (e.contains("transfer") && e.contains("Error(Contract, #10)"))
            {
                "Insufficient balance: your wallet does not hold enough of the input token \
                 for this swap amount. Lower the amount and refresh the quote."
                    .to_string()
            } else if e.contains("Error(Auth, InvalidAction)") && e.contains("approve") {
                "Swap failed: Comet pool token approval was rejected by simulation. \
                 The on-chain aggregator may need an upgrade; try refreshing the quote or a route without Comet."
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
    seq_str.parse::<i64>().map_err(|e| format!("Invalid sequence: {}", e))
}

enum TxSimulateMode {
    /// Classic PathPayment only — Soroban RPC cannot simulate these.
    Skip,
    /// Single InvokeHostFunction op.
    Full,
    /// Hybrid: simulate invoke-only clone, merge into multi-op envelope.
    InvokeOnly(String),
}

/// Soroban `simulateTransaction` accepts exactly one operation per request.
fn tx_simulate_mode(tx_xdr: &str) -> Result<TxSimulateMode, String> {
    use stellar_xdr::curr::ReadXdr;

    let envelope = xdr::TransactionEnvelope::from_xdr_base64(tx_xdr, Limits::none())
        .map_err(|e| format!("Failed to parse tx: {:?}", e))?;

    let xdr::TransactionEnvelope::Tx(v1) = envelope else {
        return Err("Unsupported transaction envelope".to_string());
    };

    let mut invoke_op: Option<xdr::Operation> = None;
    for op in v1.tx.operations.iter() {
        if matches!(op.body, xdr::OperationBody::InvokeHostFunction(_)) {
            if invoke_op.is_some() {
                return Err("Multiple Soroban invoke operations are not supported".to_string());
            }
            invoke_op = Some(op.clone());
        }
    }

    let Some(invoke_op) = invoke_op else {
        return Ok(TxSimulateMode::Skip);
    };

    if v1.tx.operations.len() == 1 {
        return Ok(TxSimulateMode::Full);
    }

    let sim_tx = xdr::Transaction {
        source_account: v1.tx.source_account.clone(),
        fee: v1.tx.fee,
        seq_num: v1.tx.seq_num,
        cond: v1.tx.cond.clone(),
        memo: v1.tx.memo.clone(),
        operations: vec![invoke_op].try_into().map_err(|_| "invoke op error".to_string())?,
        ext: xdr::TransactionExt::V0,
    };
    let sim_envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
        tx: sim_tx,
        signatures: xdr::VecM::default(),
    });
    let sim_xdr = sim_envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("Failed to encode invoke-only tx: {:?}", e))?;
    Ok(TxSimulateMode::InvokeOnly(sim_xdr))
}

async fn rpc_simulate_transaction(rpc_url: &str, tx_xdr: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "simulateTransaction",
        "params": {
            "transaction": tx_xdr,
            "resourceConfig": {
                "instructionLeeway": 3_000_000
            }
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

    if let Some(err) = resp_json.get("error") {
        return Err(format!("RPC error: {}", err));
    }

    resp_json
        .get("result")
        .cloned()
        .ok_or_else(|| "No result in simulate response".to_string())
}

fn merge_simulate_result_into_tx(full_tx_xdr: &str, result: &serde_json::Value) -> Result<String, String> {
    use stellar_xdr::curr::ReadXdr;

    if let Some(error) = result.get("error") {
        return Err(format!("Simulation failed: {}", error));
    }

    let transaction_data = result
        .get("transactionData")
        .and_then(|t| t.as_str())
        .ok_or_else(|| "No transactionData in simulate response".to_string())?;

    let mut envelope = xdr::TransactionEnvelope::from_xdr_base64(full_tx_xdr, Limits::none())
        .map_err(|e| format!("Failed to parse tx: {:?}", e))?;

    let xdr::TransactionEnvelope::Tx(ref mut v1) = envelope else {
        return Err("Unsupported transaction envelope".to_string());
    };

    let soroban_data = xdr::SorobanTransactionData::from_xdr_base64(transaction_data, Limits::none())
        .map_err(|e| format!("Failed to parse soroban data: {:?}", e))?;
    v1.tx.ext = xdr::TransactionExt::V1(soroban_data);

    if let Some(min_fee) = result
        .get("minResourceFee")
        .and_then(|f| f.as_str())
        .and_then(|f| f.parse::<u32>().ok())
    {
        v1.tx.fee = v1.tx.fee.max(min_fee + 100_000);
    }

    if let Some(results_arr) = result.get("results").and_then(|r| r.as_array()) {
        let mut ops_vec: Vec<xdr::Operation> = v1.tx.operations.to_vec();
        let mut result_idx = 0usize;
        for op in ops_vec.iter_mut() {
            if let xdr::OperationBody::InvokeHostFunction(ref mut ihf) = op.body {
                let auth_arr = results_arr
                    .get(result_idx)
                    .and_then(|r| r.get("auth"))
                    .and_then(|a| a.as_array());
                result_idx += 1;
                if let Some(auth_arr) = auth_arr {
                    let mut auth_entries = Vec::new();
                    for auth_xdr in auth_arr {
                        if let Some(auth_str) = auth_xdr.as_str() {
                            if let Ok(entry) = xdr::SorobanAuthorizationEntry::from_xdr_base64(auth_str, Limits::none())
                            {
                                auth_entries.push(entry);
                            }
                        }
                    }
                    if !auth_entries.is_empty() {
                        ihf.auth = auth_entries.try_into().unwrap_or_default();
                    }
                }
            }
        }
        v1.tx.operations = ops_vec.try_into().unwrap_or_default();
    }

    envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("Failed to encode assembled tx: {:?}", e))
}

/// Simulate transaction and assemble with footprint + auth.
async fn simulate_and_assemble(rpc_url: &str, tx_xdr: &str) -> Result<String, String> {
    match tx_simulate_mode(tx_xdr)? {
        TxSimulateMode::Skip => Ok(tx_xdr.to_string()),
        TxSimulateMode::Full => {
            let result = rpc_simulate_transaction(rpc_url, tx_xdr).await?;
            merge_simulate_result_into_tx(tx_xdr, &result)
        }
        TxSimulateMode::InvokeOnly(sim_xdr) => {
            let result = rpc_simulate_transaction(rpc_url, &sim_xdr).await?;
            merge_simulate_result_into_tx(tx_xdr, &result)
        }
    }
}

#[cfg(test)]
mod classic_dest_min_tests {
    use super::classic_dest_min_for_sub;

    #[test]
    fn single_leg_gets_full_min_out() {
        let dest = classic_dest_min_for_sub(2_000_000, 2_000_000, 150_000).unwrap();
        assert_eq!(dest, 150_000);
    }

    #[test]
    fn split_legs_allocate_proportionally() {
        let total_min = 1_000_000i128;
        let a = classic_dest_min_for_sub(600_000, 1_000_000, total_min).unwrap();
        let b = classic_dest_min_for_sub(400_000, 1_000_000, total_min).unwrap();
        assert_eq!(a, 600_000);
        assert_eq!(b, 400_000);
        assert!(a > 0 && b > 0);
    }

    #[test]
    fn rejects_zero_min_out() {
        assert!(classic_dest_min_for_sub(1, 1, 0).is_err());
    }
}

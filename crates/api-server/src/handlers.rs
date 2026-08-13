use {
    crate::{soroban_prepare::prepare_transaction_xdr, state::AppState},
    axum::{
        extract::{Query, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    },
    router_engine::{
        apply_on_chain_hop_validation,
        types::{RouteRequest, TokenId},
    },
    serde::{Deserialize, Serialize},
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
            "ready": "/api/v1/ready",
            "quote": "/api/v1/quote",
            "build_tx": "/api/v1/build_tx",
            "tokens": "/api/v1/tokens",
            "balance": "/api/v1/balance",
            "balances": "/api/v1/balances",
            "account": "/api/v1/account",
            "classic_asset": "/api/v1/classic_asset",
            "ledger_latest": "/api/v1/ledger/latest",
            "submit_tx": "/api/v1/submit_tx",
            "tx_status": "/api/v1/tx_status",
            "stats": "/api/v1/stats",
            "arbitrage": "/api/v1/arbitrage",
            "swaps": "/api/v1/swaps",
            "orders": "/api/v1/orders",
            "build_create_order": "/api/v1/orders/build_create",
            "build_cancel_order": "/api/v1/orders/build_cancel",
            "prices": "/api/v1/prices",
            "price_history": "/api/v1/prices/history"
        },
        "docs": {
            "openapi": "https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/openapi.yaml",
            "integrator_guide": "https://github.com/Lum-Agg/stellar-dex-agg/blob/main/docs/integrator-guide.md"
        },
        "rate_limits": {
            "anonymous": "10 requests/second per IP",
            "partner": "60 requests/second per X-API-Key (contact team for key)"
        },
        "repository": "https://github.com/Lum-Agg/stellar-dex-agg"
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
    /// When `1`, Soroban AMMs only (no Classic SDEX / no Horizon).
    /// Default may still return a pure classic route; mixed classic+Soroban
    /// hops are never returned (unexecutable as one tx).
    pub prefer_soroban: Option<u8>,
    /// Path-finder hop limit (pool hops). Omit = server default.
    pub max_hops: Option<usize>,
    /// Max parallel sub-routes in a split quote. Omit = server default.
    pub max_splits: Option<usize>,
    /// When `1`, re-quote selected hops via on-chain pool math (slower; for arb
    /// / diagnostics). Omit = use server env `QUOTE_ON_CHAIN_VALIDATE`
    /// (default off).
    pub on_chain_validate: Option<u8>,
}

fn clamp_route_limits(config: &crate::config::AppConfig, query: &QuoteQuery) -> (Option<usize>, Option<usize>) {
    (
        query.max_hops.map(|value| value.min(config.path_finder_max_hops)),
        query.max_splits.map(|value| value.min(config.max_splits)),
    )
}

#[cfg(test)]
mod quote_limit_tests {
    use super::{clamp_route_limits, QuoteQuery};

    #[test]
    fn quote_limits_cannot_exceed_server_bounds() {
        let config = crate::config::AppConfig {
            path_finder_max_hops: 3,
            max_splits: 3,
            ..crate::config::AppConfig::default()
        };
        let query = QuoteQuery {
            token_in: "XLM".into(),
            token_out: "USDC".into(),
            amount_in: "1".into(),
            slippage: None,
            debug: None,
            prefer_soroban: None,
            max_hops: Some(usize::MAX),
            max_splits: Some(usize::MAX),
            on_chain_validate: None,
        };
        assert_eq!(clamp_route_limits(&config, &query), (Some(3), Some(3)));
    }
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
    /// True when this response applied on-chain hop validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_chain_validated: Option<bool>,
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
        Ok(v) if v > 0 => v,
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
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(QuoteResponse {
                    success: false,
                    data: None,
                    error: Some("amount_in must be positive".to_string()),
                }),
            );
        }
    };

    let slippage = params.slippage.unwrap_or(0.5);
    if !slippage.is_finite() || !(0.0..100.0).contains(&slippage) {
        return (
            StatusCode::BAD_REQUEST,
            Json(QuoteResponse {
                success: false,
                data: None,
                error: Some("slippage must be between 0 (inclusive) and 100 (exclusive)".to_string()),
            }),
        );
    }
    let slippage_bps = (slippage * 100.0).round() as u32;
    let include_debug = params.debug.unwrap_or(0) != 0;
    let on_chain_validate = match params.on_chain_validate {
        Some(v) => v != 0,
        None => std::env::var("QUOTE_ON_CHAIN_VALIDATE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    };

    // Client-supplied route limits are useful for integrators, but must not be
    // allowed to bypass the server's configured CPU bounds.
    let (max_hops, max_splits) = clamp_route_limits(&state.config, &params);

    let request = RouteRequest {
        token_in: TokenId::from_str_auto(&params.token_in),
        token_out: TokenId::from_str_auto(&params.token_out),
        amount_in,
        slippage_bps: Some(slippage_bps),
        max_hops,
        max_splits,
        prefer_soroban: params.prefer_soroban.map(|v| v != 0),
    };

    let engine = state.current_engine().await;
    let mut route = state.quote_route(&request).await;

    if on_chain_validate && !route.sub_orders.is_empty() {
        let before = route.total_expected_out;
        route = apply_on_chain_hop_validation(&state.rpc, &engine, route, slippage_bps).await;
        tracing::info!(
            on_chain_validate = true,
            before_out = before,
            after_out = route.total_expected_out,
            delta = before as i128 - route.total_expected_out as i128,
            "quote on-chain hop validation"
        );
    }

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
                on_chain_validated: on_chain_validate.then_some(true),
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

/// Drop dust legs (< `MIN_DISPLAY_LEG_INPUT_BPS` of total input) and fold their
/// `amount_in` into the largest kept leg. Matches frontend route display and
/// avoids burning Soroban CPU on near-zero parallel paths.
const BUILD_MIN_LEG_INPUT_BPS: u128 = 10;

fn fold_dust_sub_routes(body: &BuildTxRequest) -> Result<BuildTxRequest, String> {
    if body.sub_routes.len() < 2 {
        return Ok(body.clone());
    }
    let amount_in: u128 = body.amount_in.parse().map_err(|_| "Invalid amount_in".to_string())?;
    if amount_in == 0 {
        return Ok(body.clone());
    }
    let min_in = amount_in.saturating_mul(BUILD_MIN_LEG_INPUT_BPS) / 10_000;

    let mut kept: Vec<BuildTxSubRoute> = Vec::new();
    let mut dust_sum: u128 = 0;
    for sub in &body.sub_routes {
        let leg: u128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        if leg >= min_in {
            kept.push(sub.clone());
        } else {
            dust_sum = dust_sum.saturating_add(leg);
        }
    }
    if dust_sum == 0 || kept.is_empty() {
        return Ok(body.clone());
    }

    let largest_idx = kept
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| s.amount_in.parse::<u128>().unwrap_or(0))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let largest_amt: u128 = kept[largest_idx]
        .amount_in
        .parse()
        .map_err(|_| "Invalid sub-route amount_in".to_string())?;
    kept[largest_idx].amount_in = (largest_amt.saturating_add(dust_sum)).to_string();

    Ok(BuildTxRequest {
        user_public_key: body.user_public_key.clone(),
        amount_in: body.amount_in.clone(),
        token_in: body.token_in.clone(),
        token_out: body.token_out.clone(),
        min_amount_out: body.min_amount_out.clone(),
        sub_routes: kept,
    })
}

fn validate_build_tx_request(body: &BuildTxRequest) -> Result<(), String> {
    let amount_in: i128 = body.amount_in.parse().map_err(|_| "Invalid amount_in".to_string())?;
    if amount_in <= 0 {
        return Err("amount_in must be positive".to_string());
    }

    let min_amount_out: i128 = body
        .min_amount_out
        .parse()
        .map_err(|_| "Invalid min_amount_out".to_string())?;
    if min_amount_out <= 0 {
        return Err("min_amount_out must be positive".to_string());
    }
    if body.sub_routes.is_empty() {
        return Err("At least one sub-route is required".to_string());
    }

    let mut sub_routes_total = 0i128;
    for (route_index, sub) in body.sub_routes.iter().enumerate() {
        let leg_amount: i128 = sub
            .amount_in
            .parse()
            .map_err(|_| format!("Invalid sub-route amount_in: {}", sub.amount_in))?;
        if leg_amount <= 0 {
            return Err(format!("sub-route {} amount_in must be positive", route_index + 1));
        }
        sub_routes_total = sub_routes_total
            .checked_add(leg_amount)
            .ok_or_else(|| "sub-route amount_in sum exceeds i128".to_string())?;

        let first = sub
            .steps
            .first()
            .ok_or_else(|| format!("sub-route {} must have at least one step", route_index + 1))?;
        let last = sub.steps.last().expect("first step already checked");
        if first.token_in != body.token_in {
            return Err(format!("sub-route {} does not start with token_in", route_index + 1));
        }
        if last.token_out != body.token_out {
            return Err(format!("sub-route {} does not end with token_out", route_index + 1));
        }
        for pair in sub.steps.windows(2) {
            if pair[0].token_out != pair[1].token_in {
                return Err(format!("sub-route {} has a disconnected token path", route_index + 1));
            }
        }
    }

    if sub_routes_total != amount_in {
        return Err(format!(
            "sub_routes amount_in sum ({}) does not match amount_in ({})",
            sub_routes_total, amount_in
        ));
    }
    Ok(())
}

pub async fn build_tx_impl(body: &BuildTxRequest, rpc: &dex_adapters::rpc::SorobanRpc) -> Result<BuildTxData, String> {
    use stellar_xdr::{
        curr as xdr,
        curr::{Limits, WriteXdr},
    };

    let body = fold_dust_sub_routes(body)?;
    let body = &body;
    validate_build_tx_request(body)?;

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
    let seq_num = fetch_sequence_number(rpc, &body.user_public_key).await?;
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

    // Use a default SorobanRpc for fetching sequence (test/debug only).
    let rpc = dex_adapters::rpc::SorobanRpc::mainnet();

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
    let seq_num = fetch_sequence_number(&rpc, &body.user_public_key).await?;
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
    /// Self-hosted logo URL (`https://api.lumagg.xyz/logos/...`) when enriched; empty otherwise.
    pub logo: String,
    /// `"official"` for SEP-42 downloaded icons, `"fallback"` for generated
    /// letter avatars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_kind: Option<String>,
}

fn resolve_token_logo(metadata_logo: Option<String>) -> String {
    metadata_logo.unwrap_or_default()
}

fn resolve_logo_kind(kind: Option<dex_adapters::token_metadata::LogoKind>) -> Option<String> {
    kind.map(|k| match k {
        dex_adapters::token_metadata::LogoKind::Official => "official".to_string(),
        dex_adapters::token_metadata::LogoKind::Fallback => "fallback".to_string(),
    })
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
                    return TokenInfo {
                        id: addr,
                        symbol: meta.symbol.clone(),
                        name: meta.name.clone(),
                        logo: resolve_token_logo(meta.logo.clone()),
                        logo_kind: resolve_logo_kind(meta.logo_kind),
                    };
                }
            }

            // Handle classic asset format "CODE:ISSUER" and "native"
            if addr == "native" {
                TokenInfo {
                    id: addr,
                    symbol: "XLM".to_string(),
                    name: "Stellar Lumens".to_string(),
                    logo: String::new(),
                    logo_kind: None,
                }
            } else if addr.contains(':') {
                let code = addr.split(':').next().unwrap_or(&addr).to_string();
                TokenInfo {
                    id: addr,
                    symbol: code.clone(),
                    name: code,
                    logo: String::new(),
                    logo_kind: None,
                }
            } else if let Some(meta) = metadata.get(&addr) {
                // Use metadata even if "Unknown" (at least has the short symbol)
                TokenInfo {
                    id: addr,
                    symbol: meta.symbol.clone(),
                    name: meta.name.clone(),
                    logo: resolve_token_logo(meta.logo.clone()),
                    logo_kind: resolve_logo_kind(meta.logo_kind),
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
                    logo: String::new(),
                    logo_kind: None,
                }
            }
        })
        .collect();

    Json(TokensResponse { tokens })
}

// ============================================================
// GET /api/v1/balance & /api/v1/balances
// ============================================================

#[derive(Deserialize)]
pub struct BalanceQuery {
    pub account: String,
    pub token: String,
}

#[derive(Deserialize)]
pub struct BalancesQuery {
    pub account: String,
    /// `common` = curated hubs (~15, fast). `catalog` = full quote-engine SACs.
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Serialize)]
pub struct BalanceResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    /// Classic trustline exists for this SAC (native XLM always true). Omitted
    /// when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_trustline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct BalancesResponse {
    pub success: bool,
    pub account: String,
    /// `catalog` = quote-engine SACs ∪ curated common list (Soroban RPC).
    pub scope: String,
    pub tokens_queried: Vec<String>,
    pub balances: std::collections::HashMap<String, String>,
    /// Per-token classic trustline presence (via SAC balance simulate). Omitted
    /// when unknown.
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub has_trustline: std::collections::HashMap<String, bool>,
    pub updated_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const BALANCE_FETCH_CONCURRENCY: usize = 64;
const NATIVE_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

fn is_native_sac(token: &str) -> bool {
    token == NATIVE_SAC || token == "native"
}

fn simulate_indicates_missing_trustline(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("trustline entry is missing") || e.contains("trustline is missing")
}

async fn fetch_sac_balance_with_trustline(
    rpc: &dex_adapters::rpc::SorobanRpc,
    token: &str,
    account_arg: &xdr::ScVal,
) -> (u128, Option<bool>) {
    if is_native_sac(token) {
        let balance = match rpc.simulate_call(token, "balance", vec![account_arg.clone()]).await {
            Ok(val) => scval_to_i128(&val).unwrap_or(0).max(0) as u128,
            Err(_) => 0,
        };
        return (balance, Some(true));
    }

    match rpc.simulate_call(token, "balance", vec![account_arg.clone()]).await {
        Ok(val) => {
            let balance = scval_to_i128(&val).unwrap_or(0).max(0) as u128;
            (balance, Some(true))
        }
        Err(e) => {
            if simulate_indicates_missing_trustline(&e.to_string()) {
                (0, Some(false))
            } else {
                (0, None)
            }
        }
    }
}

fn scval_to_i128(val: &xdr::ScVal) -> Option<i128> {
    match val {
        xdr::ScVal::I128(parts) => Some(((parts.hi as i128) << 64) | (parts.lo as i128)),
        _ => None,
    }
}

fn parse_account_public_key(account: &str) -> Result<stellar_strkey::ed25519::PublicKey, String> {
    stellar_strkey::ed25519::PublicKey::from_string(account.trim()).map_err(|_| "Invalid account address".to_string())
}

fn account_balance_scval(user_key: &stellar_strkey::ed25519::PublicKey) -> xdr::ScVal {
    xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
        xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
    )))
}

pub(crate) fn collect_common_balance_token_ids() -> Vec<String> {
    dex_adapters::COMMON_BALANCE_TOKEN_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

/// SAC ids to batch-balance: quote-engine catalog ∪ curated common list.
/// Soroban-only (no Horizon). Classic `CODE:ISSUER` / `native` aliases skipped.
async fn collect_catalog_balance_token_ids(state: &AppState) -> Vec<String> {
    let engine = state.current_engine().await;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for id in engine.get_all_tokens().await {
        if id.starts_with('C') && id.len() == 56 && seen.insert(id.clone()) {
            out.push(id);
        }
    }
    for id in collect_common_balance_token_ids() {
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

fn normalize_balances_scope(raw: Option<&str>) -> &'static str {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("common") => "common",
        _ => "catalog",
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn fetch_balances_for_tokens(
    rpc: std::sync::Arc<dex_adapters::rpc::SorobanRpc>,
    account_arg: &xdr::ScVal,
    token_ids: Vec<String>,
) -> (
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, bool>,
) {
    use futures::stream::{self, StreamExt};

    let mut balances = std::collections::HashMap::new();
    let mut has_trustline = std::collections::HashMap::new();

    let results: Vec<_> = stream::iter(token_ids)
        .map(|token| {
            let rpc = rpc.clone();
            let account_arg = account_arg.clone();
            async move {
                let (amount, tl) = fetch_sac_balance_with_trustline(&rpc, &token, &account_arg).await;
                (token, amount, tl)
            }
        })
        .buffer_unordered(BALANCE_FETCH_CONCURRENCY)
        .collect()
        .await;

    for (token, amount, tl) in results {
        if amount > 0 {
            balances.insert(token.clone(), amount.to_string());
        }
        if let Some(v) = tl {
            has_trustline.insert(token, v);
        }
    }

    (balances, has_trustline)
}

pub async fn get_balance(State(state): State<AppState>, Query(query): Query<BalanceQuery>) -> impl IntoResponse {
    let user_key = match parse_account_public_key(&query.account) {
        Ok(key) => key,
        Err(error) => {
            return Json(BalanceResponse {
                success: false,
                balance: None,
                has_trustline: None,
                error: Some(error),
            });
        }
    };

    let token = query.token.trim();
    if token.is_empty() {
        return Json(BalanceResponse {
            success: false,
            balance: None,
            has_trustline: None,
            error: Some("Missing token contract id".to_string()),
        });
    }

    let account_arg = account_balance_scval(&user_key);
    let (balance, has_trustline) = fetch_sac_balance_with_trustline(&state.rpc, token, &account_arg).await;

    Json(BalanceResponse {
        success: true,
        balance: Some(balance.to_string()),
        has_trustline,
        error: None,
    })
}

pub async fn get_balances(State(state): State<AppState>, Query(query): Query<BalancesQuery>) -> impl IntoResponse {
    let scope = normalize_balances_scope(query.scope.as_deref());
    let account = query.account.trim().to_string();

    let user_key = match parse_account_public_key(&account) {
        Ok(key) => key,
        Err(error) => {
            return Json(BalancesResponse {
                success: false,
                account: query.account,
                scope: scope.to_string(),
                tokens_queried: vec![],
                balances: std::collections::HashMap::new(),
                has_trustline: std::collections::HashMap::new(),
                updated_at_ms: 0,
                error: Some(error),
            });
        }
    };

    let token_ids = match scope {
        "common" => collect_common_balance_token_ids(),
        _ => collect_catalog_balance_token_ids(&state).await,
    };
    let tokens_queried = token_ids.clone();
    let account_arg = account_balance_scval(&user_key);
    let (balances, has_trustline) = fetch_balances_for_tokens(state.rpc.clone(), &account_arg, token_ids).await;

    Json(BalancesResponse {
        success: true,
        account,
        scope: scope.to_string(),
        tokens_queried,
        balances,
        has_trustline,
        updated_at_ms: now_ms(),
        error: None,
    })
}

// ============================================================
// GET /api/v1/classic_asset — SAC contract → classic code/issuer
// ============================================================

#[derive(Deserialize)]
pub struct ClassicAssetQuery {
    /// SAC contract id (`C…`).
    pub contract: String,
}

#[derive(Serialize)]
pub struct ClassicAssetResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Parse stellar.expert `asset` field: `CODE-GISSUER[-domain]`.
pub(crate) fn parse_expert_asset_field(asset: &str) -> Option<(String, String)> {
    let (code, rest) = asset.split_once('-')?;
    if code.is_empty() || code.len() > 12 {
        return None;
    }
    let issuer = rest
        .split('-')
        .find(|p| p.len() == 56 && p.starts_with('G'))?
        .to_string();
    if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((code.to_string(), issuer))
}

/// Resolve a Stellar Asset Contract to its classic code + issuer (for
/// ChangeTrust).
pub async fn get_classic_asset(Query(query): Query<ClassicAssetQuery>) -> impl IntoResponse {
    let contract = query.contract.trim();
    if contract.is_empty() {
        return Json(ClassicAssetResponse {
            success: false,
            code: None,
            issuer: None,
            error: Some("Missing contract id".to_string()),
        });
    }
    if is_native_sac(contract) {
        return Json(ClassicAssetResponse {
            success: false,
            code: None,
            issuer: None,
            error: Some("Native XLM does not require a trustline".to_string()),
        });
    }
    if !contract.starts_with('C') || contract.len() != 56 {
        return Json(ClassicAssetResponse {
            success: false,
            code: None,
            issuer: None,
            error: Some("Expected a 56-character SAC contract id (C…)".to_string()),
        });
    }

    // Fast path for well-known SACs (same set as classic DEX adapter).
    const KNOWN: &[(&str, &str, &str)] = &[
        (
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            "USDC",
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        ),
        (
            "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
            "EURC",
            "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
        ),
    ];
    for (sac, code, issuer) in KNOWN {
        if *sac == contract {
            return Json(ClassicAssetResponse {
                success: true,
                code: Some((*code).to_string()),
                issuer: Some((*issuer).to_string()),
                error: None,
            });
        }
    }

    let url = format!("https://api.stellar.expert/explorer/public/contract/{}", contract);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Json(ClassicAssetResponse {
                success: false,
                code: None,
                issuer: None,
                error: Some(format!("HTTP client error: {e}")),
            });
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    return Json(ClassicAssetResponse {
                        success: false,
                        code: None,
                        issuer: None,
                        error: Some(format!("Invalid expert response: {e}")),
                    });
                }
            };
            let Some(asset_str) = body.get("asset").and_then(|v| v.as_str()) else {
                return Json(ClassicAssetResponse {
                    success: false,
                    code: None,
                    issuer: None,
                    error: Some("Contract is not a classic SAC (no linked asset); trustline N/A".to_string()),
                });
            };
            match parse_expert_asset_field(asset_str) {
                Some((code, issuer)) => Json(ClassicAssetResponse {
                    success: true,
                    code: Some(code),
                    issuer: Some(issuer),
                    error: None,
                }),
                None => Json(ClassicAssetResponse {
                    success: false,
                    code: None,
                    issuer: None,
                    error: Some(format!("Unrecognized asset encoding: {asset_str}")),
                }),
            }
        }
        Ok(resp) => Json(ClassicAssetResponse {
            success: false,
            code: None,
            issuer: None,
            error: Some(format!("Asset lookup failed ({})", resp.status().as_u16())),
        }),
        Err(e) => Json(ClassicAssetResponse {
            success: false,
            code: None,
            issuer: None,
            error: Some(format!("Asset lookup error: {e}")),
        }),
    }
}

// ============================================================
// GET /api/v1/account, /api/v1/ledger/latest, POST /api/v1/submit_tx
// (Soroban RPC proxy — keeps RPC_URL server-side)
// ============================================================

#[derive(Deserialize)]
pub struct AccountQuery {
    pub account: String,
}

#[derive(Serialize)]
pub struct AccountResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn get_account(State(state): State<AppState>, Query(query): Query<AccountQuery>) -> impl IntoResponse {
    let account = query.account.trim();
    if account.is_empty() {
        return Json(AccountResponse {
            success: false,
            sequence: None,
            error: Some("Missing account address".to_string()),
        });
    }

    match fetch_sequence_number(&state.rpc, account).await {
        Ok(seq) => Json(AccountResponse {
            success: true,
            sequence: Some(seq.to_string()),
            error: None,
        }),
        Err(error) => Json(AccountResponse {
            success: false,
            sequence: None,
            error: Some(error),
        }),
    }
}

#[derive(Serialize)]
pub struct LatestLedgerResponse {
    pub success: bool,
    pub sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn get_latest_ledger(State(state): State<AppState>) -> impl IntoResponse {
    match state.rpc.get_latest_ledger().await {
        Ok(ledger) => Json(LatestLedgerResponse {
            success: true,
            sequence: ledger.sequence as u64,
            error: None,
        }),
        Err(e) => Json(LatestLedgerResponse {
            success: false,
            sequence: 0,
            error: Some(e.to_string()),
        }),
    }
}

#[derive(Deserialize)]
pub struct SubmitTxRequest {
    pub signed_tx_xdr: String,
}

#[derive(Serialize)]
pub struct SubmitTxResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const MAX_SIGNED_TX_XDR_BYTES: usize = 96 * 1024;

fn validate_signed_tx_xdr(signed_tx_xdr: &str) -> Result<(), String> {
    if signed_tx_xdr.len() > MAX_SIGNED_TX_XDR_BYTES {
        return Err(format!(
            "signed_tx_xdr exceeds the {} byte limit",
            MAX_SIGNED_TX_XDR_BYTES
        ));
    }

    use stellar_xdr::curr::{Limits, ReadXdr, TransactionEnvelope};
    let envelope = TransactionEnvelope::from_xdr_base64(signed_tx_xdr, Limits::none())
        .map_err(|_| "signed_tx_xdr is not valid TransactionEnvelope XDR".to_string())?;
    match envelope {
        TransactionEnvelope::Tx(tx) if !tx.signatures.is_empty() => Ok(()),
        TransactionEnvelope::Tx(_) => Err("signed_tx_xdr has no signatures".to_string()),
        TransactionEnvelope::TxV0(_) => Err("transaction v0 envelopes are not supported".to_string()),
        TransactionEnvelope::TxFeeBump(_) => Err("fee-bump envelopes are not supported".to_string()),
    }
}

pub async fn submit_tx(State(state): State<AppState>, Json(body): Json<SubmitTxRequest>) -> impl IntoResponse {
    let signed_tx_xdr = body.signed_tx_xdr.trim();
    if signed_tx_xdr.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(SubmitTxResponse {
                success: false,
                hash: None,
                status: None,
                error: Some("Missing signed_tx_xdr".to_string()),
            }),
        );
    }

    if let Err(error) = validate_signed_tx_xdr(signed_tx_xdr) {
        return (
            StatusCode::BAD_REQUEST,
            Json(SubmitTxResponse {
                success: false,
                hash: None,
                status: None,
                error: Some(error),
            }),
        );
    }

    // Fast return: enqueue only. Clients poll `/api/v1/tx_status` (or balance) for
    // inclusion. Do not retry TRY_AGAIN_LATER here: the caller owns retry policy
    // and may decide whether the signed transaction is still fresh enough.
    match state.rpc.send_transaction(signed_tx_xdr).await {
            Ok(result) => {
                let accepted_status = result.status == "PENDING" || result.status == "DUPLICATE";
                let accepted = accepted_status && !result.hash.is_empty();
                let response_status = if accepted {
                    StatusCode::OK
                } else if result.status == "TRY_AGAIN_LATER" {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_REQUEST
                };
                return (
                    response_status,
                    Json(SubmitTxResponse {
                        success: accepted,
                        hash: if result.hash.is_empty() {
                            None
                        } else {
                            Some(result.hash)
                        },
                        status: Some(result.status.clone()),
                        error: if accepted {
                            None
                        } else if accepted_status {
                            Some("Transaction accepted without a transaction hash".to_string())
                        } else if result.status == "TRY_AGAIN_LATER" {
                            Some("Transaction queue is temporarily unavailable; retry shortly".to_string())
                        } else {
                            Some(
                                result
                                    .error_result_xdr
                                    .unwrap_or_else(|| format!("Transaction rejected (status={})", result.status)),
                            )
                        },
                    }),
                );
            }
            Err(e) => (
                StatusCode::BAD_GATEWAY,
                Json(SubmitTxResponse {
                    success: false,
                    hash: None,
                    status: None,
                    error: Some(e.to_string()),
                }),
            ),
        }
}

#[derive(Deserialize)]
pub struct TxStatusQuery {
    pub hash: String,
}

#[derive(Serialize)]
pub struct TxStatusResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// `SUCCESS` | `FAILED` | `NOT_FOUND` | …
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// True only when status is `SUCCESS`.
    pub confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/v1/tx_status?hash=… — poll after fast `/api/v1/submit_tx`.
pub async fn get_tx_status(State(state): State<AppState>, Query(query): Query<TxStatusQuery>) -> impl IntoResponse {
    let hash = query.hash.trim();
    if hash.is_empty() {
        return Json(TxStatusResponse {
            success: false,
            hash: None,
            status: None,
            confirmed: false,
            error: Some("Missing hash".to_string()),
        });
    }

    match state.rpc.get_transaction(hash).await {
        Ok(result) => {
            let confirmed = result.status == "SUCCESS";
            let failed = result.status == "FAILED";
            Json(TxStatusResponse {
                success: true,
                hash: Some(hash.to_string()),
                status: Some(result.status.clone()),
                confirmed,
                error: if failed {
                    Some("Transaction failed on-chain".to_string())
                } else {
                    None
                },
            })
        }
        Err(e) => Json(TxStatusResponse {
            success: false,
            hash: Some(hash.to_string()),
            status: None,
            confirmed: false,
            error: Some(e.to_string()),
        }),
    }
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

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub ready: bool,
    pub tokens: usize,
    pub pools: usize,
}

async fn routing_graph_status(engine: &router_engine::QuoteEngine) -> (usize, usize, bool) {
    let tokens = engine.get_all_tokens().await.len();
    let pools = engine.cached_pool_edges().await.len();
    (tokens, pools, tokens > 1 && pools > 0)
}

/// Readiness is separate from liveness: a newly started embedded instance can
/// accept HTTP connections before its first market snapshot is available.
pub async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.engine.read().await.clone();
    let (tokens, pools, ready) = routing_graph_status(&engine).await;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(ReadinessResponse {
            status: if ready { "ready" } else { "warming_up" }.to_string(),
            ready,
            tokens,
            pools,
        }),
    )
}

#[cfg(test)]
mod readiness_tests {
    use {
        super::routing_graph_status,
        router_engine::{
            path_finder::PathFinderConfig,
            split_optimizer::SplitConfig,
            types::{TokenId, TradingPair},
            QuoteEngine,
        },
    };

    #[tokio::test]
    async fn readiness_requires_a_populated_routing_graph() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        assert_eq!(routing_graph_status(&engine).await, (0, 0, false));

        engine
            .update_pairs_from_cache(
                "soroswap",
                &[TradingPair {
                    token_a: TokenId::from_str_auto("TOKEN_A"),
                    token_b: TokenId::from_str_auto("TOKEN_B"),
                    source: "soroswap".into(),
                    pool_address: "POOL".into(),
                    fee_bps: 30,
                    reserve_a: Some(1_000_000),
                    reserve_b: Some(1_000_000),
                }],
            )
            .await;

        assert_eq!(routing_graph_status(&engine).await, (2, 1, true));
    }
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
#[derive(Clone, Deserialize)]
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

    if token == "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV" {
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

pub async fn build_tx(State(state): State<AppState>, Json(body): Json<BuildTxRequest>) -> impl IntoResponse {
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

    match build_tx_impl(&body, &state.rpc).await {
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
            } else if e.contains("ExceededLimit") || e.contains("Budget") {
                "Swap simulation failed: this route is too heavy for Soroban CPU limits \
                 (too many split paths / hops). Refresh the quote — a simpler route should appear."
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
            } else if e.contains("trustline entry is missing") ||
                e.contains("trustline is missing") ||
                (e.contains("Error(Contract, #13)") && e.contains("trustline"))
            {
                "Missing trustline: your wallet cannot receive the output token yet. \
                 Add a trustline for the buy asset in your wallet (e.g. USDC), then retry. \
                 Keep ~0.5 XLM free for the new trustline reserve."
                    .to_string()
            } else if e.contains("Error(Auth, InvalidAction)") && e.contains("approve") {
                "Swap failed: Comet pool token approval was rejected by simulation. \
                 The on-chain aggregator may need an upgrade; try refreshing the quote or a route without Comet."
                    .to_string()
            } else if e.contains("account not found") ||
                e.contains("No sequence") ||
                e.contains("Horizon") ||
                e.contains("rate limit") ||
                e.contains("Rate Limit")
            {
                // Sequence lookup failed before SimulateTransaction — don't label as sim
                // failure.
                e
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

/// Fetch account sequence via the app's Soroban RPC (`getLedgerEntries`).
pub(crate) async fn fetch_sequence_number(
    rpc: &dex_adapters::rpc::SorobanRpc,
    public_key: &str,
) -> Result<i64, String> {
    rpc.get_account_sequence(public_key).await.map_err(|e| {
        format!(
            "Could not load account sequence for {public_key}: {e}. \
             Ensure the account is funded and RPC_URL is reachable."
        )
    })
}

#[cfg(test)]
mod classic_dest_min_tests {
    use super::{
        classic_dest_min_for_sub, parse_expert_asset_field, validate_build_tx_request, BuildTxRequest, BuildTxStep,
        BuildTxSubRoute,
    };

    fn valid_build_request() -> BuildTxRequest {
        BuildTxRequest {
            user_public_key: "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY".into(),
            amount_in: "100".into(),
            token_in: "TOKEN_A".into(),
            token_out: "TOKEN_C".into(),
            min_amount_out: "90".into(),
            sub_routes: vec![BuildTxSubRoute {
                amount_in: "100".into(),
                steps: vec![
                    BuildTxStep {
                        dex_type: "aquarius".into(),
                        pool_address: "POOL_1".into(),
                        token_in: "TOKEN_A".into(),
                        token_out: "TOKEN_B".into(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                    BuildTxStep {
                        dex_type: "aquarius".into(),
                        pool_address: "POOL_2".into(),
                        token_in: "TOKEN_B".into(),
                        token_out: "TOKEN_C".into(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            }],
        }
    }

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

    #[test]
    fn validates_well_formed_build_route() {
        assert!(validate_build_tx_request(&valid_build_request()).is_ok());
    }

    #[test]
    fn rejects_non_positive_build_amounts() {
        let mut request = valid_build_request();
        request.amount_in = "-100".into();
        request.sub_routes[0].amount_in = "-100".into();
        assert_eq!(
            validate_build_tx_request(&request).unwrap_err(),
            "amount_in must be positive"
        );

        let mut request = valid_build_request();
        request.min_amount_out = "0".into();
        assert_eq!(
            validate_build_tx_request(&request).unwrap_err(),
            "min_amount_out must be positive"
        );
    }

    #[test]
    fn rejects_disconnected_build_route() {
        let mut request = valid_build_request();
        request.sub_routes[0].steps[1].token_in = "OTHER_TOKEN".into();
        assert_eq!(
            validate_build_tx_request(&request).unwrap_err(),
            "sub-route 1 has a disconnected token path"
        );
    }

    #[test]
    fn parses_expert_asset_field() {
        assert_eq!(
            parse_expert_asset_field("FADA-GCX3Y4MNI7ZQBQEZQMAXRFVODVFB2PRQS4LTUHP5B34MEYQQTW5LQFLR-1"),
            Some((
                "FADA".to_string(),
                "GCX3Y4MNI7ZQBQEZQMAXRFVODVFB2PRQS4LTUHP5B34MEYQQTW5LQFLR".to_string()
            ))
        );
        assert_eq!(
            parse_expert_asset_field("USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN-1"),
            Some((
                "USDC".to_string(),
                "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".to_string()
            ))
        );
        assert_eq!(parse_expert_asset_field("native"), None);
    }
}

#[cfg(test)]
mod submit_tx_validation_tests {
    use super::validate_signed_tx_xdr;

    #[test]
    fn rejects_invalid_xdr_before_rpc() {
        assert_eq!(
            validate_signed_tx_xdr("not-xdr").unwrap_err(),
            "signed_tx_xdr is not valid TransactionEnvelope XDR"
        );
    }

    #[test]
    fn rejects_oversized_xdr_before_decode() {
        let oversized = "A".repeat(super::MAX_SIGNED_TX_XDR_BYTES + 1);
        assert!(validate_signed_tx_xdr(&oversized)
            .unwrap_err()
            .contains("exceeds"));
    }
}

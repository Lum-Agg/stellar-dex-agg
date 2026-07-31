//! Wallet-scoped limit orders from analytics-indexer SQLite and escrow
//! transaction builders.

use {
    crate::{
        handlers::{fetch_sequence_number, BuildTxData, BuildTxResponse},
        soroban_prepare::{network_passphrase_from_env, prepare_transaction_xdr_on_network},
        state::AppState,
    },
    analytics_indexer::store::IndexStore,
    axum::{
        extract::{rejection::JsonRejection, Query, State},
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    serde::{Deserialize, Serialize},
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr as xdr,
};

#[derive(Debug, Deserialize)]
pub struct OrdersQuery {
    pub user: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OrdersResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<OrdersData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OrdersData {
    pub orders: Vec<OrderItem>,
}

#[derive(Debug, Serialize)]
pub struct OrderItem {
    pub order_id: i64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_initial: Option<String>,
    pub amount_in_remaining: String,
    pub limit_out_per_in_e7: String,
    pub expires_ledger: u32,
    pub status: String,
    pub created_ledger: Option<u32>,
    pub updated_ledger: u32,
    pub created_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct BuildCreateRequest {
    pub user: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub limit_out_per_in_e7: String,
    pub expires_ledger: u32,
}

#[derive(Debug, Deserialize)]
pub struct BuildCancelRequest {
    pub user: String,
    pub order_id: u64,
}

#[derive(Debug, Serialize)]
pub struct DcaOrdersData {
    pub orders: Vec<DcaOrderItem>,
}

#[derive(Debug, Serialize)]
pub struct DcaOrderItem {
    pub order_id: i64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_initial: String,
    pub amount_in_remaining: String,
    pub chunk_amount: String,
    pub interval_ledgers: u32,
    pub next_executable_ledger: u32,
    pub min_out_per_in_e7: String,
    pub expires_ledger: u32,
    pub status: String,
    pub updated_ledger: u32,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct BuildCreateDcaRequest {
    pub user: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub chunk_amount: String,
    pub interval_ledgers: u32,
    pub start_ledger: u32,
    pub min_out_per_in_e7: String,
    pub expires_ledger: u32,
}

fn indexer_db_path() -> Option<String> {
    std::env::var("INDEXER_DB_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("LUMAGG_INDEXER_DB_PATH").ok().filter(|s| !s.is_empty()))
}

pub async fn get_orders(Query(params): Query<OrdersQuery>) -> Response {
    let Some(user) = params.user.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("missing required query param: user".into()),
            }),
        )
            .into_response();
    };
    if PublicKey::from_string(user).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("user must be a Stellar G... address".into()),
            }),
        )
            .into_response();
    }

    let status_filter = match params.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("open") => None,
        Some("all") => Some("all"),
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OrdersResponse {
                    success: false,
                    data: None,
                    error: Some("status must be open or all".into()),
                }),
            )
                .into_response();
        }
    };

    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(OrdersResponse {
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
                StatusCode::SERVICE_UNAVAILABLE,
                Json(OrdersResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };

    match store.list_by_owner(user, status_filter) {
        Ok(rows) => {
            let orders = rows
                .into_iter()
                .map(|r| OrderItem {
                    order_id: r.order_id,
                    owner: r.owner,
                    token_in: r.token_in,
                    token_out: r.token_out,
                    amount_in_initial: r.amount_in_initial,
                    amount_in_remaining: r.amount_in_remaining,
                    limit_out_per_in_e7: r.limit_out_per_in_e7,
                    expires_ledger: r.expires_ledger,
                    status: r.status,
                    created_ledger: r.created_ledger,
                    updated_ledger: r.updated_ledger,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                })
                .collect();
            (
                StatusCode::OK,
                Json(OrdersResponse {
                    success: true,
                    data: Some(OrdersData { orders }),
                    error: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}

pub async fn get_dca_orders(Query(params): Query<OrdersQuery>) -> Response {
    let Some(user) = params.user.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("missing required query param: user".into()),
            }),
        )
            .into_response();
    };
    if PublicKey::from_string(user).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("user must be a Stellar G... address".into()),
            }),
        )
            .into_response();
    }
    let include_all = matches!(params.status.as_deref(), Some("all"));
    if !matches!(params.status.as_deref(), None | Some("open") | Some("all")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("status must be open or all".into()),
            }),
        )
            .into_response();
    }
    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(OrdersResponse {
                success: false,
                data: None,
                error: Some("Analytics DB not configured (set INDEXER_DB_PATH on api-server)".into()),
            }),
        )
            .into_response();
    };
    let store = match IndexStore::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(OrdersResponse {
                    success: false,
                    data: None,
                    error: Some(error.to_string()),
                }),
            )
                .into_response()
        }
    };
    match store.list_dca_by_owner(user, include_all) {
        Ok(rows) => Json(
            serde_json::json!({ "success": true, "data": { "orders": rows.into_iter().map(|r| DcaOrderItem {
            order_id: r.order_id, owner: r.owner, token_in: r.token_in, token_out: r.token_out,
            amount_in_initial: r.amount_in_initial, amount_in_remaining: r.amount_in_remaining,
            chunk_amount: r.chunk_amount, interval_ledgers: r.interval_ledgers,
            next_executable_ledger: r.next_executable_ledger, min_out_per_in_e7: r.min_out_per_in_e7,
            expires_ledger: r.expires_ledger, status: r.status, updated_ledger: r.updated_ledger,
            updated_at: r.updated_at,
        }).collect::<Vec<_>>() } }),
        )
        .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        )
            .into_response(),
    }
}

fn require_escrow_contract(value: Option<String>) -> Result<String, String> {
    let contract = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "ESCROW_CONTRACT is not configured".to_string())?;
    Contract::from_string(&contract)
        .map_err(|_| "ESCROW_CONTRACT must be a Stellar C... contract address".to_string())?;
    Ok(contract)
}

fn parse_user(user: &str) -> Result<PublicKey, String> {
    PublicKey::from_string(user.trim()).map_err(|_| "user must be a Stellar G... address".to_string())
}

fn parse_contract(value: &str, field: &str) -> Result<[u8; 32], String> {
    Contract::from_string(value.trim())
        .map(|contract| contract.0)
        .map_err(|_| format!("{field} must be a Stellar C... contract address"))
}

fn parse_positive_i128(value: &str, field: &str) -> Result<i128, String> {
    let value: i128 = value
        .trim()
        .parse()
        .map_err(|_| format!("{field} must be a positive integer"))?;
    if value <= 0 {
        return Err(format!("{field} must be greater than zero"));
    }
    Ok(value)
}

fn parse_nonnegative_i128(value: &str, field: &str) -> Result<i128, String> {
    let value: i128 = value
        .trim()
        .parse()
        .map_err(|_| format!("{field} must be a non-negative integer"))?;
    if value < 0 {
        return Err(format!("{field} must not be negative"));
    }
    Ok(value)
}

fn validate_create_request(request: &BuildCreateRequest) -> Result<(), String> {
    parse_user(&request.user)?;
    parse_contract(&request.token_in, "token_in")?;
    parse_contract(&request.token_out, "token_out")?;
    if request.token_in.trim() == request.token_out.trim() {
        return Err("token_in and token_out must differ".to_string());
    }
    parse_positive_i128(&request.amount_in, "amount_in")?;
    parse_positive_i128(&request.limit_out_per_in_e7, "limit_out_per_in_e7")?;
    if request.expires_ledger == 0 {
        return Err("expires_ledger must be greater than zero".to_string());
    }
    Ok(())
}

fn validate_cancel_request(request: &BuildCancelRequest) -> Result<(), String> {
    parse_user(&request.user).map(|_| ())
}

fn account_scval(user_key: &PublicKey) -> xdr::ScVal {
    xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
        xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(user_key.0)),
    )))
}

fn contract_scval(contract: [u8; 32]) -> xdr::ScVal {
    xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract))))
}

fn i128_scval(value: i128) -> xdr::ScVal {
    xdr::ScVal::I128(xdr::Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

fn contract_operation(contract: &str, function: &str, args: Vec<xdr::ScVal>) -> Result<xdr::Operation, String> {
    let contract = parse_contract(contract, "ESCROW_CONTRACT")?;
    Ok(xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
            host_function: xdr::HostFunction::InvokeContract(xdr::InvokeContractArgs {
                contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract))),
                function_name: xdr::ScSymbol(
                    function
                        .try_into()
                        .map_err(|_| "invalid contract function name".to_string())?,
                ),
                args: args.try_into().map_err(|_| "too many contract arguments".to_string())?,
            }),
            auth: xdr::VecM::default(),
        }),
    })
}

fn build_create_operation(contract: &str, request: &BuildCreateRequest) -> Result<xdr::Operation, String> {
    validate_create_request(request)?;
    let user = parse_user(&request.user)?;
    let token_in = parse_contract(&request.token_in, "token_in")?;
    let token_out = parse_contract(&request.token_out, "token_out")?;
    let amount_in = parse_positive_i128(&request.amount_in, "amount_in")?;
    let limit = parse_positive_i128(&request.limit_out_per_in_e7, "limit_out_per_in_e7")?;

    contract_operation(
        contract,
        "create_limit",
        vec![
            account_scval(&user),
            contract_scval(token_in),
            contract_scval(token_out),
            i128_scval(amount_in),
            i128_scval(limit),
            xdr::ScVal::U32(request.expires_ledger),
        ],
    )
}

fn build_cancel_operation(contract: &str, request: &BuildCancelRequest) -> Result<xdr::Operation, String> {
    validate_cancel_request(request)?;
    contract_operation(contract, "cancel", vec![xdr::ScVal::U64(request.order_id)])
}

fn build_create_dca_operation(contract: &str, request: &BuildCreateDcaRequest) -> Result<xdr::Operation, String> {
    let user = parse_user(&request.user)?;
    let token_in = parse_contract(&request.token_in, "token_in")?;
    let token_out = parse_contract(&request.token_out, "token_out")?;
    if token_in == token_out {
        return Err("token_in and token_out must differ".into());
    }
    let amount_in = parse_positive_i128(&request.amount_in, "amount_in")?;
    let chunk = parse_positive_i128(&request.chunk_amount, "chunk_amount")?;
    if chunk > amount_in {
        return Err("chunk_amount must not exceed amount_in".into());
    }
    if request.interval_ledgers == 0 {
        return Err("interval_ledgers must be positive".into());
    }
    if request.expires_ledger <= request.start_ledger {
        return Err("expires_ledger must follow start_ledger".into());
    }
    let min_rate = parse_nonnegative_i128(&request.min_out_per_in_e7, "min_out_per_in_e7")?;
    contract_operation(
        contract,
        "create_dca",
        vec![
            account_scval(&user),
            contract_scval(token_in),
            contract_scval(token_out),
            i128_scval(amount_in),
            i128_scval(chunk),
            xdr::ScVal::U32(request.interval_ledgers),
            xdr::ScVal::U32(request.start_ledger),
            i128_scval(min_rate),
            xdr::ScVal::U32(request.expires_ledger),
        ],
    )
}

fn build_cancel_dca_operation(contract: &str, request: &BuildCancelRequest) -> Result<xdr::Operation, String> {
    validate_cancel_request(request)?;
    contract_operation(contract, "cancel_dca", vec![xdr::ScVal::U64(request.order_id)])
}

async fn prepare_order_transaction(
    rpc: &dex_adapters::rpc::SorobanRpc,
    user: &str,
    contract: String,
    operation: xdr::Operation,
) -> Result<BuildTxData, String> {
    const SOROBAN_FEE: u32 = 100_000;

    let sequence = fetch_sequence_number(rpc, user).await?;
    let rpc_url = rpc.url().to_string();
    let network = network_passphrase_from_env();
    let unsigned_tx_xdr = prepare_transaction_xdr_on_network(
        &rpc_url,
        user.trim(),
        sequence as u64,
        &[operation],
        SOROBAN_FEE,
        network,
    )
    .await?;

    Ok(BuildTxData {
        unsigned_tx_xdr,
        num_operations: 1,
        fee: SOROBAN_FEE.to_string(),
        contract,
        execution: "soroban".to_string(),
    })
}

fn order_error(status: StatusCode, error: String) -> Response {
    (
        status,
        Json(BuildTxResponse {
            success: false,
            data: None,
            error: Some(error),
        }),
    )
        .into_response()
}

fn order_build_failure(error: String) -> Response {
    (
        StatusCode::OK,
        Json(BuildTxResponse {
            success: false,
            data: None,
            error: Some(error),
        }),
    )
        .into_response()
}

pub async fn build_create(
    State(state): State<AppState>,
    request: Result<Json<BuildCreateRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, format!("invalid request body: {error}")),
    };
    if let Err(error) = validate_create_request(&request) {
        return order_error(StatusCode::BAD_REQUEST, error);
    }
    let contract = match require_escrow_contract(std::env::var("ESCROW_CONTRACT").ok()) {
        Ok(contract) => contract,
        Err(error) => return order_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let operation = match build_create_operation(&contract, &request) {
        Ok(operation) => operation,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error),
    };

    match prepare_order_transaction(&state.rpc, &request.user, contract, operation).await {
        Ok(data) => (
            StatusCode::OK,
            Json(BuildTxResponse {
                success: true,
                data: Some(data),
                error: None,
            }),
        )
            .into_response(),
        Err(error) => order_build_failure(format!("Order build failed: {error}")),
    }
}

pub async fn build_cancel(
    State(state): State<AppState>,
    request: Result<Json<BuildCancelRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, format!("invalid request body: {error}")),
    };
    if let Err(error) = validate_cancel_request(&request) {
        return order_error(StatusCode::BAD_REQUEST, error);
    }
    let contract = match require_escrow_contract(std::env::var("ESCROW_CONTRACT").ok()) {
        Ok(contract) => contract,
        Err(error) => return order_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let operation = match build_cancel_operation(&contract, &request) {
        Ok(operation) => operation,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error),
    };

    match prepare_order_transaction(&state.rpc, &request.user, contract, operation).await {
        Ok(data) => (
            StatusCode::OK,
            Json(BuildTxResponse {
                success: true,
                data: Some(data),
                error: None,
            }),
        )
            .into_response(),
        Err(error) => order_build_failure(format!("Order build failed: {error}")),
    }
}

pub async fn build_create_dca(
    State(state): State<AppState>,
    request: Result<Json<BuildCreateDcaRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error.body_text()),
    };
    let contract = match require_escrow_contract(std::env::var("ESCROW_CONTRACT").ok()) {
        Ok(contract) => contract,
        Err(error) => return order_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let operation = match build_create_dca_operation(&contract, &request) {
        Ok(operation) => operation,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error),
    };
    match prepare_order_transaction(&state.rpc, &request.user, contract, operation).await {
        Ok(data) => Json(BuildTxResponse {
            success: true,
            data: Some(data),
            error: None,
        })
        .into_response(),
        Err(error) => order_build_failure(error),
    }
}

pub async fn build_cancel_dca(
    State(state): State<AppState>,
    request: Result<Json<BuildCancelRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error.body_text()),
    };
    let contract = match require_escrow_contract(std::env::var("ESCROW_CONTRACT").ok()) {
        Ok(contract) => contract,
        Err(error) => return order_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let operation = match build_cancel_dca_operation(&contract, &request) {
        Ok(operation) => operation,
        Err(error) => return order_error(StatusCode::BAD_REQUEST, error),
    };
    match prepare_order_transaction(&state.rpc, &request.user, contract, operation).await {
        Ok(data) => Json(BuildTxResponse {
            success: true,
            data: Some(data),
            error: None,
        })
        .into_response(),
        Err(error) => order_build_failure(error),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{config::AppConfig, state::AppState},
        analytics_indexer::store::IndexStore,
        axum::{extract::State, http::StatusCode, response::IntoResponse, Json},
        serde_json::Value,
        tempfile::tempdir,
    };

    const TEST_USER: &str = "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY";

    fn seed_db(path: &std::path::Path) {
        let store = IndexStore::open(path).unwrap();
        store
            .upsert_created(
                1,
                TEST_USER,
                "TIN",
                "TOUT",
                "1000000",
                "1000000",
                "5000000",
                500,
                10,
                10,
                1_700_000_000,
                1_700_000_000,
            )
            .unwrap();
    }

    async fn body_json(resp: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn missing_user_is_400() {
        let resp = get_orders(Query(OrdersQuery {
            user: None,
            status: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_user_is_400() {
        let resp = get_orders(Query(OrdersQuery {
            user: Some("not-an-address".into()),
            status: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_status_is_400() {
        let resp = get_orders(Query(OrdersQuery {
            user: Some(TEST_USER.into()),
            status: Some("closed".into()),
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn no_db_env_is_503() {
        std::env::remove_var("INDEXER_DB_PATH");
        std::env::remove_var("LUMAGG_INDEXER_DB_PATH");
        let resp = get_orders(Query(OrdersQuery {
            user: Some(TEST_USER.into()),
            status: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn unavailable_db_is_503() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing").join("idx.db");
        std::env::set_var("INDEXER_DB_PATH", path.to_str().unwrap());
        let resp = get_orders(Query(OrdersQuery {
            user: Some(TEST_USER.into()),
            status: None,
        }))
        .await
        .into_response();
        std::env::remove_var("INDEXER_DB_PATH");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn returns_rows_when_db_configured() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("idx.db");
        seed_db(&path);
        std::env::set_var("INDEXER_DB_PATH", path.to_str().unwrap());
        let resp = get_orders(Query(OrdersQuery {
            user: Some(TEST_USER.into()),
            status: Some("open".into()),
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["orders"].as_array().unwrap().len(), 1);
        assert_eq!(json["data"]["orders"][0]["order_id"], 1);
        assert_eq!(json["data"]["orders"][0]["token_in"], "TIN");
        std::env::remove_var("INDEXER_DB_PATH");
    }

    const TEST_TOKEN_IN: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
    const TEST_TOKEN_OUT: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    const TEST_ESCROW: &str = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";

    fn valid_create_request() -> BuildCreateRequest {
        BuildCreateRequest {
            user: TEST_USER.into(),
            token_in: TEST_TOKEN_IN.into(),
            token_out: TEST_TOKEN_OUT.into(),
            amount_in: "10000000".into(),
            limit_out_per_in_e7: "20000000".into(),
            expires_ledger: 12_345_678,
        }
    }

    async fn dummy_app_state() -> AppState {
        AppState::new(AppConfig::default()).await.unwrap()
    }

    #[tokio::test]
    async fn build_create_invalid_user_is_400() {
        let mut request = valid_create_request();
        request.user = "not-an-address".into();
        let resp = build_create(State(dummy_app_state().await), Ok(Json(request)))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert_eq!(json["success"], false);
    }

    #[tokio::test]
    async fn build_create_missing_escrow_contract_is_503() {
        std::env::remove_var("ESCROW_CONTRACT");
        let resp = build_create(State(dummy_app_state().await), Ok(Json(valid_create_request())))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let json = body_json(resp).await;
        assert_eq!(json["success"], false);
    }

    #[test]
    fn create_validation_rejects_invalid_user_and_non_positive_amount() {
        let mut request = valid_create_request();
        request.user = "not-an-address".into();
        assert!(validate_create_request(&request).is_err());

        let mut request = valid_create_request();
        request.amount_in = "0".into();
        assert!(validate_create_request(&request).is_err());
    }

    #[test]
    fn missing_escrow_contract_is_an_error() {
        assert!(require_escrow_contract(None).is_err());
        assert!(require_escrow_contract(Some("".into())).is_err());
    }

    #[test]
    fn create_and_cancel_operations_target_escrow_abi() {
        use stellar_xdr::curr as xdr;

        let create = build_create_operation(TEST_ESCROW, &valid_create_request()).unwrap();
        let cancel = build_cancel_operation(
            TEST_ESCROW,
            &BuildCancelRequest {
                user: TEST_USER.into(),
                order_id: 0,
            },
        )
        .unwrap();

        let invoke_contract = |operation: xdr::Operation| match operation.body {
            xdr::OperationBody::InvokeHostFunction(invoke) => match invoke.host_function {
                xdr::HostFunction::InvokeContract(args) => args,
                _ => panic!("expected contract invocation"),
            },
            _ => panic!("expected invoke host function"),
        };

        let create_args = invoke_contract(create);
        assert_eq!(create_args.function_name.to_string(), "create_limit");
        assert_eq!(create_args.args.len(), 6);

        let cancel_args = invoke_contract(cancel);
        assert_eq!(cancel_args.function_name.to_string(), "cancel");
        assert_eq!(cancel_args.args.len(), 1);
        assert!(matches!(cancel_args.args.first(), Some(xdr::ScVal::U64(0))));
    }
}

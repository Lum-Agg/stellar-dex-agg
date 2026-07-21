//! Wallet-scoped swap history from analytics-indexer SQLite.

use {
    analytics_indexer::store::IndexStore,
    axum::{
        extract::Query,
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    },
    serde::{Deserialize, Serialize},
    stellar_strkey::ed25519::PublicKey,
};

#[derive(Debug, Deserialize)]
pub struct SwapsQuery {
    pub user: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SwapsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SwapsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SwapsData {
    pub swaps: Vec<SwapItem>,
}

#[derive(Debug, Serialize)]
pub struct SwapItem {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub function_name: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub is_split: bool,
}

fn indexer_db_path() -> Option<String> {
    std::env::var("INDEXER_DB_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("LUMAGG_INDEXER_DB_PATH").ok().filter(|s| !s.is_empty()))
}

pub async fn get_swaps(Query(params): Query<SwapsQuery>) -> Response {
    let Some(user) = params.user.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(SwapsResponse {
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
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some("user must be a Stellar G... address".into()),
            }),
        )
            .into_response();
    }

    let Some(db_path) = indexer_db_path() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SwapsResponse {
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
                Json(SwapsResponse {
                    success: false,
                    data: None,
                    error: Some(format!("open indexer db: {e}")),
                }),
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    match store.list_swaps_by_user(user, limit) {
        Ok(rows) => {
            let swaps = rows
                .into_iter()
                .map(|r| SwapItem {
                    tx_hash: r.tx_hash,
                    ledger: r.ledger,
                    created_at: r.created_at,
                    status: r.status,
                    function_name: r.function_name,
                    token_in: r.token_in,
                    token_out: r.token_out,
                    amount_in: r.amount_in,
                    amount_out: r.amount_out,
                    is_split: r.is_split,
                })
                .collect();
            (
                StatusCode::OK,
                Json(SwapsResponse {
                    success: true,
                    data: Some(SwapsData { swaps }),
                    error: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SwapsResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        analytics_indexer::{
            parser::ParsedInvocation,
            store::{IndexStore, StoredInvocation},
        },
        axum::{http::StatusCode, response::IntoResponse},
        serde_json::Value,
        tempfile::tempdir,
    };

    const TEST_USER: &str = "GA6RKSBPI2TSP52OW2IJTPK7LRMX24DF42KF3FBGBNMBYCV6NPDMOCBY";

    fn seed_db(path: &std::path::Path) {
        let store = IndexStore::open(path).unwrap();
        let _ = store.insert_invocation(&StoredInvocation {
            tx_hash: "abc".into(),
            ledger: 10,
            created_at: 1_700_000_000,
            status: "SUCCESS".into(),
            parsed: ParsedInvocation {
                function_name: "swap".into(),
                user_address: TEST_USER.into(),
                token_in: Some("TIN".into()),
                token_out: Some("TOUT".into()),
                amount_in: 1_000_0000,
                amount_out: Some(2_000_0000),
                is_split: false,
                legs: vec![],
            },
        });
    }

    async fn body_json(resp: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn missing_user_is_400() {
        let resp = get_swaps(Query(SwapsQuery {
            user: None,
            limit: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_user_is_400() {
        let resp = get_swaps(Query(SwapsQuery {
            user: Some("not-an-address".into()),
            limit: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_checksum_g_address_is_400() {
        let resp = get_swaps(Query(SwapsQuery {
            user: Some("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into()),
            limit: None,
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn no_db_env_is_503() {
        std::env::remove_var("INDEXER_DB_PATH");
        std::env::remove_var("LUMAGG_INDEXER_DB_PATH");
        let resp = get_swaps(Query(SwapsQuery {
            user: Some(TEST_USER.into()),
            limit: None,
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
        let resp = get_swaps(Query(SwapsQuery {
            user: Some(TEST_USER.into()),
            limit: None,
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
        let resp = get_swaps(Query(SwapsQuery {
            user: Some(TEST_USER.into()),
            limit: Some(20),
        }))
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["swaps"].as_array().unwrap().len(), 1);
        assert_eq!(json["data"]["swaps"][0]["tx_hash"], "abc");
        std::env::remove_var("INDEXER_DB_PATH");
    }
}

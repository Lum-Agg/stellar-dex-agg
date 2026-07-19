//! Wallet-scoped limit orders from analytics-indexer SQLite.

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

#[cfg(test)]
mod tests {
    use super::*;
    use analytics_indexer::store::IndexStore;
    use axum::{
        http::StatusCode,
        response::IntoResponse,
    };
    use serde_json::Value;
    use tempfile::tempdir;

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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
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
}

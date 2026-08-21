pub mod arbitrage;
pub mod config;
pub mod handlers;
pub mod orders;
pub mod pool_hydrate;
pub mod price_mark;
pub mod price_sampler;
pub mod price_store;
pub mod prices;
pub mod rate_limit;
pub mod snapshot_loader;
pub mod soroban_prepare;
pub mod state;
pub mod stats;
pub mod swaps;
pub mod xlm_price;

use {
    axum::{
        middleware,
        routing::{get, post},
        Router,
    },
    config::AppConfig,
    rate_limit::RateLimitState,
    state::AppState,
    std::{net::SocketAddr, path::PathBuf},
    tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, services::ServeDir},
    tracing::info,
};

fn build_router(app_state: AppState, rate_limit: RateLimitState, logo_dir: PathBuf) -> Router {
    let api = Router::new()
        .route("/", get(handlers::api_root))
        .route("/api/v1/quote", get(handlers::get_quote))
        .route("/api/v1/build_tx", post(handlers::build_tx))
        .route("/api/v1/tokens", get(handlers::list_tokens))
        .route("/api/v1/balance", get(handlers::get_balance))
        .route("/api/v1/balances", get(handlers::get_balances))
        .route("/api/v1/account", get(handlers::get_account))
        .route("/api/v1/classic_asset", get(handlers::get_classic_asset))
        .route("/api/v1/ledger/latest", get(handlers::get_latest_ledger))
        .route("/api/v1/submit_tx", post(handlers::submit_tx))
        .route("/api/v1/tx_status", get(handlers::get_tx_status))
        .route("/api/v1/health", get(handlers::health_check))
        .route("/api/v1/ready", get(handlers::readiness_check))
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/arbitrage", get(arbitrage::get_arbitrage))
        .route("/api/v1/swaps", get(swaps::get_swaps))
        .route("/api/v1/orders", get(orders::get_orders))
        .route("/api/v1/orders/build_create", post(orders::build_create))
        .route("/api/v1/orders/build_cancel", post(orders::build_cancel))
        .route("/api/v1/dca", get(orders::get_dca_orders))
        .route("/api/v1/dca/build_create", post(orders::build_create_dca))
        .route("/api/v1/dca/build_cancel", post(orders::build_cancel_dca))
        .route("/api/v1/prices", get(prices::get_prices))
        .route("/api/v1/prices/history", get(prices::get_price_history))
        .layer(middleware::from_fn_with_state(
            rate_limit,
            rate_limit::rate_limit_middleware,
        ))
        .with_state(app_state);

    // Logos are static assets and must not consume API rate-limit quota.
    Router::new()
        .merge(api)
        .nest_service("/logos", ServeDir::new(logo_dir))
        // Signed Stellar transactions are small; reject oversized JSON before
        // deserialization or forwarding it to the upstream RPC.
        .layer(RequestBodyLimitLayer::new(128 * 1024))
        .layer(CorsLayer::permissive())
}

pub async fn run_server() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    info!(
        "Config: mode={:?}, rpc_url={}, listen={}, discovery={}s, refresh={}s",
        config.lumagg_mode,
        config.rpc_url,
        config.listen_addr,
        config.discovery_interval_secs,
        config.refresh_interval_secs
    );

    let listen_addr: SocketAddr = config.listen_addr.parse()?;
    let app_state = AppState::new(config).await?;
    let rate_limit = RateLimitState::from_env();
    let logo_dir = PathBuf::from(std::env::var("TOKEN_LOGO_DIR").unwrap_or_else(|_| "data/logos".into()));
    std::fs::create_dir_all(&logo_dir)?;
    let app = build_router(app_state, rate_limit, logo_dir);

    info!("Stellar DEX Aggregator API listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        axum::http::{header::CONTENT_TYPE, Request, StatusCode},
        tower::ServiceExt,
    };

    #[tokio::test]
    async fn serves_logo_files_from_configured_directory() {
        let dir = std::env::temp_dir().join(format!(
            "lumagg-logo-serve-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = dir.join("sample.svg");
        std::fs::write(&sample, b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();

        let app = axum::Router::new().nest_service("/logos", tower_http::services::ServeDir::new(&dir));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/logos/sample.svg")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(content_type.starts_with("image/svg+xml"), "got {content_type}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

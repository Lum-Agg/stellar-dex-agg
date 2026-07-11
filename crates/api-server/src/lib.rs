pub mod config;
pub mod handlers;
pub mod pool_hydrate;
pub mod rate_limit;
pub mod snapshot_loader;
pub mod soroban_prepare;
pub mod state;

use {
    axum::{
        middleware,
        routing::{get, post},
        Router,
    },
    config::AppConfig,
    state::AppState,
    std::net::SocketAddr,
    tower_http::cors::CorsLayer,
    tracing::info,
};

pub async fn run_server() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    info!(
        "Config: rpc_url={}, listen={}, discovery={}s, refresh={}s",
        config.rpc_url, config.listen_addr, config.discovery_interval_secs, config.refresh_interval_secs
    );

    let listen_addr: SocketAddr = config.listen_addr.parse()?;
    let app_state = AppState::new(config).await?;
    let rate_limit = rate_limit::RateLimitState::from_env();

    let app = Router::new()
        .route("/", get(handlers::api_root))
        .route("/api/v1/quote", get(handlers::get_quote))
        .route("/api/v1/build_tx", post(handlers::build_tx))
        .route("/api/v1/tokens", get(handlers::list_tokens))
        .route("/api/v1/balance", get(handlers::get_balance))
        .route("/api/v1/balances", get(handlers::get_balances))
        .route("/api/v1/health", get(handlers::health_check))
        .layer(middleware::from_fn_with_state(
            rate_limit.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    info!("Stellar DEX Aggregator API listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

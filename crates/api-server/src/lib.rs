pub mod config;
pub mod handlers;
pub mod pool_hydrate;
pub mod snapshot_loader;
pub mod soroban_prepare;
pub mod state;

use axum::{routing::get, routing::post, Router};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing::info;

use config::AppConfig;
use state::AppState;

pub async fn run_server() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    info!(
        "Config: rpc_url={}, listen={}, discovery={}s, refresh={}s",
        config.rpc_url,
        config.listen_addr,
        config.discovery_interval_secs,
        config.refresh_interval_secs
    );

    let listen_addr: SocketAddr = config.listen_addr.parse()?;
    let app_state = AppState::new(config).await?;

    let app = Router::new()
        .route("/api/v1/quote", get(handlers::get_quote))
        .route("/api/v1/build_tx", post(handlers::build_tx))
        .route("/api/v1/tokens", get(handlers::list_tokens))
        .route("/api/v1/health", get(handlers::health_check))
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    info!("Stellar DEX Aggregator API listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

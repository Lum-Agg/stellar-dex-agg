use api_server::run_server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=info,router_engine=info,dex_adapters=info".into()),
        )
        .init();

    if let Err(e) = run_server().await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}

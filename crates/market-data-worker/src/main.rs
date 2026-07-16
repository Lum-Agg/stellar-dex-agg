use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    market_data_worker::worker::run(market_data_worker::worker::WorkerConfig::from_env()?).await
}

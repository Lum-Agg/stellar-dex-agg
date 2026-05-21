mod worker;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    worker::run(worker::WorkerConfig::from_env()?).await
}

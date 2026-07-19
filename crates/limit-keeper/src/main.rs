use {
    anyhow::{bail, Result},
    limit_keeper::config::KeeperConfig,
    tracing::info,
    tracing_subscriber::EnvFilter,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("limit_keeper=info".parse()?))
        .init();

    let config = KeeperConfig::from_env()?;
    info!(dry_run = config.dry_run, "loaded limit keeper configuration");

    bail!("limit-keeper is not fully implemented")
}

use {
    anyhow::{bail, Context, Result},
    lumagg_config::aggregator::AggregatorConfig,
    std::path::PathBuf,
};

fn print_help() {
    println!(
        "lumagg-market-data-worker {}\n\n\
         Usage: lumagg-market-data-worker --config <FILE> [--check-config]\n\n\
         Options:\n  --config <FILE>  LumAgg Aggregator TOML configuration\n  \
         --check-config  Validate configuration and exit\n  -h, --help      Show this help\n  \
         -V, --version   Show version",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<Option<(Option<PathBuf>, bool)>> {
    let mut args = std::env::args().skip(1);
    let mut config = None;
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(args.next().context("--config requires a file path")?.into()),
            "--check-config" => check = true,
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("lumagg-market-data-worker {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Some((config, check)))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some((path, check)) = parse_args()? else {
        return Ok(());
    };
    if let Some(path) = path {
        let config: AggregatorConfig = lumagg_config::load(&path)?;
        config.validate_cluster()?;
        config.apply();
    } else if check {
        bail!("--check-config requires --config <FILE>");
    }
    if check {
        println!("configuration is valid");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "market_data_worker=info,dex_adapters=info".into()),
        )
        .init();
    market_data_worker::worker::run(market_data_worker::worker::WorkerConfig::from_env()?).await
}

use {
    anyhow::{bail, Context, Result},
    lumagg_config::aggregator::AggregatorConfig,
    std::{path::PathBuf, process::ExitCode},
};

struct Args {
    config: Option<PathBuf>,
    listen_addr: Option<String>,
    check_config: bool,
}

fn print_help() {
    println!(
        "lumagg-swap-api {}\n\n\
         Self-hosted LumAgg API with an embedded market-data worker.\n\n\
         Usage: lumagg-swap-api --config <FILE> [--listen-addr <ADDR>] [--check-config]\n\n\
         Options:\n  --config <FILE>       LumAgg TOML configuration\n  \
         --listen-addr <ADDR> Override api.listen_addr\n  --check-config       Validate and exit\n  \
         -h, --help           Show help\n  -V, --version        Show version",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        config: None,
        listen_addr: None,
        check_config: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => parsed.config = Some(args.next().context("--config requires a file path")?.into()),
            "--listen-addr" => parsed.listen_addr = Some(args.next().context("--listen-addr requires a value")?),
            "--check-config" => parsed.check_config = true,
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("lumagg-swap-api {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Some(parsed))
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    if let Some(path) = args.config {
        let config: AggregatorConfig = lumagg_config::load(&path)?;
        config.validate_embedded()?;
        config.apply();
    } else if args.check_config {
        bail!("--check-config requires --config <FILE>");
    }
    if let Some(addr) = args.listen_addr {
        std::env::set_var("LISTEN_ADDR", addr);
    }
    if args.check_config {
        println!("configuration is valid");
        return Ok(());
    }

    std::env::set_var("LUMAGG_MODE", "embedded");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "lumagg_swap_api=info,api_server=info,market_data_worker=info,router_engine=info,dex_adapters=info"
                    .into()
            }),
        )
        .init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting LumAgg Swap API");
    api_server::run_server().await
}

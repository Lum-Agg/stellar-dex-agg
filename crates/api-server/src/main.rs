use {
    anyhow::{bail, Context, Result},
    api_server::run_server,
    lumagg_config::aggregator::AggregatorConfig,
    std::path::PathBuf,
};

struct Args {
    config: Option<PathBuf>,
    listen_addr: Option<String>,
    check_config: bool,
}

fn print_help() {
    println!(
        "lumagg-api-server {}\n\n\
         Usage: lumagg-api-server --config <FILE> [--listen-addr <ADDR>] [--check-config]\n\n\
         Options:\n  --config <FILE>       LumAgg Aggregator TOML configuration\n  \
         --listen-addr <ADDR> Override api.listen_addr for this replica\n  \
         --check-config       Validate configuration and exit\n  -h, --help           Show this help\n  -V, --version        Show version",
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
                println!("lumagg-api-server {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Some(parsed))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    if let Some(path) = args.config {
        let config: AggregatorConfig = lumagg_config::load(&path)?;
        config.validate_cluster()?;
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

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=info,router_engine=info,dex_adapters=info".into()),
        )
        .init();
    run_server().await
}

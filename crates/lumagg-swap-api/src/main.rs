use std::process::ExitCode;

fn print_help() {
    println!(
        "lumagg-swap-api {}\n\n\
         Self-hosted LumAgg quote and transaction-build API.\n\
         The market-data worker runs in the same process; Redis is not required.\n\n\
         Usage:\n  lumagg-swap-api\n  lumagg-swap-api --help\n  lumagg-swap-api --version\n\n\
         Required configuration:\n  RPC_URL=<Stellar Soroban RPC URL>\n\n\
         Common optional configuration:\n  LISTEN_ADDR=0.0.0.0:3100\n  AGGREGATOR_CONTRACT=<C... contract id>\n  RUST_LOG=info\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<bool, String> {
    let mut args = std::env::args().skip(1);
    let Some(arg) = args.next() else {
        return Ok(true);
    };
    if args.next().is_some() {
        return Err("lumagg-swap-api does not accept positional arguments".into());
    }
    match arg.as_str() {
        "-h" | "--help" => {
            print_help();
            Ok(false)
        }
        "-V" | "--version" => {
            println!("lumagg-swap-api {}", env!("CARGO_PKG_VERSION"));
            Ok(false)
        }
        _ => Err(format!("unknown argument: {arg}")),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let should_run = match parse_args() {
        Ok(should_run) => should_run,
        Err(error) => {
            eprintln!("error: {error}\n\nRun 'lumagg-swap-api --help' for usage.");
            return ExitCode::from(2);
        }
    };
    if !should_run {
        return ExitCode::SUCCESS;
    }

    // This distribution is intentionally all-in-one. Production cluster
    // deployments continue to run api-server and market-data-worker separately.
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
    match api_server::run_server().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, "LumAgg Swap API exited");
            ExitCode::FAILURE
        }
    }
}

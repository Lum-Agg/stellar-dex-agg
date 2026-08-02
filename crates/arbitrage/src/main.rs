//! LumAgg round-trip arbitrage bot (quote-api + aggregator.round_trip_swap).
//!
//! Burberry pipeline: BridgeCollector → parallel workers → async TxExecutor.
//!
//!   ARB_BRIDGE_TOKENS=... ARB_AGGREGATOR_CONTRACT=C... ARB_QUOTE_API_URL=http://127.0.0.1:8080 \
//!   ARB_BUILD_TX=1 ARB_SUBMIT_TX=1 cargo run -p arbitrage --bin
//! lumagg-arbitrage-bot

use {
    anyhow::{bail, Context, Result},
    arbitrage::{pipeline::engine, telegram, ArbConfig, ArbRuntime},
    lumagg_config::arbitrage::ArbitrageConfig,
    std::{path::PathBuf, sync::Arc},
    tracing::info,
    tracing_subscriber::EnvFilter,
};

fn print_help() {
    println!(
        "lumagg-arbitrage-bot {}\n\n\
         Usage: lumagg-arbitrage-bot --config <FILE> [--check-config]\n\n\
         Options:\n  --config <FILE>  LumAgg Arbitrage TOML configuration\n  \
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
                println!("lumagg-arbitrage-bot {}", env!("CARGO_PKG_VERSION"));
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
        let config: ArbitrageConfig = lumagg_config::load(&path)?;
        config.validate()?;
        config.apply();
    } else if check {
        bail!("--check-config requires --config <FILE>");
    }
    if check {
        println!("configuration is valid");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = ArbConfig::from_env()?;
    let runtime = Arc::new(ArbRuntime::from_config(config)?);
    runtime.log_startup();

    if let Some(alerter) = lumagg_alerts::TelegramAlerter::from_env().map(Arc::new) {
        info!("Telegram profit reports + quiet-window alerts enabled");
        let _ = alerter.send("LumAgg arbitrage bot started (burberry pipeline)").await;
        telegram::spawn_hourly_profit_report(runtime.clone(), alerter.clone());
        telegram::spawn_quiet_window_monitor(runtime.clone(), alerter);
    } else {
        info!("Telegram disabled (set TELEGRAM_ALERTS_ENABLED + token/chat in telegram.env)");
    }

    engine::start_bot(runtime).await
}

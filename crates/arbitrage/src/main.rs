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
         Usage: lumagg-arbitrage-bot --config <FILE> [--check-config]\n\
         Read diagnostics: lumagg-arbitrage-bot --diagnostics-db <FILE>\n\n\
         Options:\n  --config <FILE>  LumAgg Arbitrage TOML configuration\n  \
         --check-config  Validate configuration and exit\n  \
         --diagnostics-db <FILE>  Read persisted 7-day simulation diagnostics and exit\n  -h, --help      Show this help\n  \
         -V, --version   Show version",
        env!("CARGO_PKG_VERSION")
    );
}

#[derive(Debug)]
struct CliArgs {
    config: Option<PathBuf>,
    check: bool,
    diagnostics_db: Option<PathBuf>,
}

fn parse_args_from(args: impl IntoIterator<Item = String>) -> Result<Option<CliArgs>> {
    let mut args = args.into_iter();
    let mut config = None;
    let mut check = false;
    let mut diagnostics_db = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(args.next().context("--config requires a file path")?.into()),
            "--check-config" => check = true,
            "--diagnostics-db" => {
                diagnostics_db = Some(args.next().context("--diagnostics-db requires a file path")?.into())
            }
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
    if diagnostics_db.is_some() && (config.is_some() || check) {
        bail!("--diagnostics-db is a standalone read-only command; do not combine with --config or --check-config");
    }
    Ok(Some(CliArgs {
        config,
        check,
        diagnostics_db,
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args_from(std::env::args().skip(1))? else {
        return Ok(());
    };
    if let Some(path) = args.diagnostics_db {
        let report = arbitrage::diagnostics::read_report(&path)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let check = args.check;
    if let Some(path) = args.config {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<Option<CliArgs>> {
        parse_args_from(values.iter().map(|v| (*v).to_owned()))
    }

    #[test]
    fn diagnostics_is_standalone_and_requires_a_path() {
        assert_eq!(
            args(&["--diagnostics-db", "test.db"]).unwrap().unwrap().diagnostics_db,
            Some("test.db".into())
        );
        assert!(args(&["--diagnostics-db"]).is_err());
        assert!(args(&["--diagnostics-db", "test.db", "--config", "test.toml"]).is_err());
        assert!(args(&["--check-config", "--diagnostics-db", "test.db"]).is_err());
        assert!(
            args(&["--config", "test.toml", "--check-config"])
                .unwrap()
                .unwrap()
                .check
        );
    }
}

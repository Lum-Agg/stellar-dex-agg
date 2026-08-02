use {
    analytics_indexer::{export, ingest, store::IndexStore, IndexerConfig},
    anyhow::{bail, Context, Result},
    lumagg_config::aggregator::AggregatorConfig,
    std::path::PathBuf,
};

#[derive(Debug)]
struct Args {
    config: Option<PathBuf>,
    command: String,
    day: Option<String>,
    start_ledger: Option<u32>,
    repair_from_ts: i64,
    check_config: bool,
}

fn print_help() {
    println!(
        "lumagg-analytics-indexer {}\n\n\
         Usage: lumagg-analytics-indexer --config <FILE> [COMMAND] [OPTIONS]\n\n\
         Commands:\n  run                         Continuously ingest events (default)\n  \
         backfill --start-ledger <N>  Backfill from a ledger and exit\n  \
         status                      Print the current database cursor\n  \
         export-daily [YYYY-MM-DD]   Export one day or all daily rollups\n  \
         repair-legs [--from-ts <N>] Repair stored leg attribution\n\n\
         Options:\n  --config <FILE>       LumAgg Aggregator TOML configuration\n  \
         --check-config       Validate configuration and exit\n  -h, --help           Show this help\n  \
         -V, --version        Show version",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut values = std::env::args().skip(1);
    let mut args = Args {
        config: None,
        command: "run".into(),
        day: None,
        start_ledger: None,
        repair_from_ts: 0,
        check_config: false,
    };
    let mut command_set = false;

    while let Some(value) = values.next() {
        match value.as_str() {
            "--config" => args.config = Some(values.next().context("--config requires a file path")?.into()),
            "--start-ledger" => {
                args.start_ledger = Some(
                    values
                        .next()
                        .context("--start-ledger requires a value")?
                        .parse()
                        .context("parse --start-ledger")?,
                );
            }
            "--from-ts" => {
                args.repair_from_ts = values
                    .next()
                    .context("--from-ts requires a value")?
                    .parse()
                    .context("parse --from-ts")?;
            }
            "--check-config" => args.check_config = true,
            "run" | "backfill" | "status" | "export-daily" | "repair-legs" if !command_set => {
                args.command = value;
                command_set = true;
            }
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("lumagg-analytics-indexer {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ if args.command == "export-daily" && args.day.is_none() => args.day = Some(value),
            _ => bail!("unknown argument: {value}"),
        }
    }
    Ok(Some(args))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let path = args.config.context("--config <FILE> is required")?;
    let aggregator: AggregatorConfig = lumagg_config::load(path)?;
    aggregator.validate_indexer()?;
    if args.check_config {
        println!("configuration is valid");
        return Ok(());
    }

    let log_filter = aggregator
        .monitoring
        .log_filter
        .clone()
        .unwrap_or_else(|| "analytics_indexer=info".into());
    let config = IndexerConfig::from_aggregator(&aggregator)?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_filter))
        .init();

    match args.command.as_str() {
        "run" => ingest::run(config).await,
        "backfill" => {
            let start = args
                .start_ledger
                .or(config.start_ledger)
                .context("backfill requires --start-ledger <N> or indexer.start_ledger")?;
            ingest::backfill(config, start).await
        }
        "export-daily" => {
            let store = IndexStore::open(&config.db_path)?;
            let stats = if let Some(day) = args.day {
                vec![export::export_daily(&store, &day)?]
            } else {
                export::export_all_days(&store)?
            };
            println!("{}", serde_json::to_string_pretty(&stats)?);
            Ok(())
        }
        "status" => {
            let store = IndexStore::open(&config.db_path)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "db_path": config.db_path,
                    "aggregator_contract": config.aggregator_contract,
                    "index_mode": config.index_mode.to_string(),
                    "envelope_fallback": config.envelope_fallback,
                    "cursor_ledger": store.cursor_ledger()?,
                    "invocation_count": store.count_invocations()?,
                    "oldest_created_at": store.oldest_created_at()?,
                }))?
            );
            Ok(())
        }
        "repair-legs" => {
            let fixed = ingest::repair_leg_indices(config, args.repair_from_ts).await?;
            println!("{{\"repaired\":{fixed}}}");
            Ok(())
        }
        command => bail!("unknown command: {command}"),
    }
}

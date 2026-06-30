use analytics_indexer::{export, ingest, store::IndexStore, IndexerConfig};
use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "analytics_indexer=info".into()),
        )
        .init();

    let config = IndexerConfig::from_env()?;
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "run".into());

    match cmd.as_str() {
        "run" => ingest::run(config).await,
        "backfill" => {
            let start: u32 = std::env::var("INDEXER_START_LEDGER")
                .context("INDEXER_START_LEDGER required for backfill")?
                .parse()
                .context("parse INDEXER_START_LEDGER")?;
            ingest::backfill(config, start).await
        }
        "export-daily" => {
            let store = IndexStore::open(&config.db_path)?;
            let day = std::env::args().nth(2);
            let stats = if let Some(d) = day {
                vec![export::export_daily(&store, &d)?]
            } else {
                export::export_all_days(&store)?
            };
            println!("{}", serde_json::to_string_pretty(&stats)?);
            Ok(())
        }
        "status" => {
            let store = IndexStore::open(&config.db_path)?;
            let cursor = store.cursor_ledger()?;
            let count = store.count_invocations()?;
            let oldest = store.oldest_created_at()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "db_path": config.db_path,
                    "aggregator_contract": config.aggregator_contract,
                    "cursor_ledger": cursor,
                    "invocation_count": count,
                    "oldest_created_at": oldest,
                }))?
            );
            Ok(())
        }
        other => {
            eprintln!(
                "usage: analytics-indexer [run|backfill|export-daily [YYYY-MM-DD]|status]"
            );
            Err(anyhow::anyhow!("unknown command: {}", other))
        }
    }
}

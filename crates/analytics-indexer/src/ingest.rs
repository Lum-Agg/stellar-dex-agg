//! Poll Soroban RPC and persist aggregator invocations.

use {
    crate::{
        config::{IndexerConfig, DEFAULT_LOOKBACK_LEDGERS},
        parser::parse_envelope,
        store::{IndexStore, StoredInvocation},
    },
    anyhow::{Context, Result},
    dex_adapters::rpc::transactions::{TransactionFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
    tracing::{info, warn},
};

pub async fn run(config: IndexerConfig) -> Result<()> {
    config.ensure_parent_dir()?;
    let store = IndexStore::open(&config.db_path)?;
    let rpc = config.rpc();

    let mut cursor = resolve_start_ledger(&store, &config, &rpc).await?;
    info!(
        aggregator = %config.aggregator_contract,
        cursor,
        db = %config.db_path,
        "analytics indexer started"
    );

    loop {
        let latest = rpc
            .get_latest_ledger()
            .await
            .context("getLatestLedger")?
            .sequence;

        if cursor >= latest {
            tokio::time::sleep(std::time::Duration::from_secs(config.poll_secs)).await;
            continue;
        }

        let end = cursor
            .saturating_add(MAX_LEDGER_SCAN_PER_REQUEST)
            .min(latest);

        match ingest_range(&config, &store, &rpc, cursor, end).await {
            Ok(ingested) => {
                store.set_cursor_ledger(end)?;
                cursor = end;
                if ingested > 0 {
                    info!(ingested, cursor, "indexed aggregator txs");
                }
            }
            Err(e) => {
                warn!(error = %e, cursor, end, "ingest batch failed");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(config.poll_secs)).await;
    }
}

async fn resolve_start_ledger(
    store: &IndexStore,
    config: &IndexerConfig,
    rpc: &dex_adapters::SorobanRpc,
) -> Result<u32> {
    if let Some(saved) = store.cursor_ledger()? {
        return Ok(saved);
    }
    if let Some(start) = config.start_ledger {
        store.set_cursor_ledger(start)?;
        return Ok(start);
    }

    let latest = rpc.get_latest_ledger().await?.sequence;
    let start = latest.saturating_sub(DEFAULT_LOOKBACK_LEDGERS);
    store.set_cursor_ledger(start)?;
    Ok(start)
}

async fn ingest_range(
    config: &IndexerConfig,
    store: &IndexStore,
    rpc: &dex_adapters::SorobanRpc,
    start_ledger: u32,
    end_ledger: u32,
) -> Result<u64> {
    let filters = vec![TransactionFilterSpec {
        contract_ids: Some(vec![config.aggregator_contract.clone()]),
    }];

    let txs = rpc
        .get_contract_transactions(
            start_ledger,
            Some(end_ledger),
            &filters,
            config.page_limit,
        )
        .await
        .with_context(|| format!("getTransactions [{start_ledger}, {end_ledger})"))?;

    let mut ingested = 0u64;
    for tx in txs {
        let parsed = match parse_envelope(
            &tx.envelope_xdr,
            &config.aggregator_contract,
            tx.result_xdr.as_deref(),
        ) {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                warn!(tx = %tx.tx_hash, error = %e, "failed to parse envelope");
                continue;
            }
        };

        let record = StoredInvocation {
            tx_hash: tx.tx_hash.clone(),
            ledger: tx.ledger,
            created_at: tx.created_at,
            status: tx.status.clone(),
            parsed,
        };

        if store.insert_invocation(&record)? {
            ingested += 1;
        }
    }

    Ok(ingested)
}

/// One-shot backfill for `[start_ledger, latest)` then exit.
pub async fn backfill(config: IndexerConfig, start_ledger: u32) -> Result<()> {
    config.ensure_parent_dir()?;
    let store = IndexStore::open(&config.db_path)?;
    let rpc = config.rpc();
    let latest = rpc.get_latest_ledger().await?.sequence;

    let mut cursor = start_ledger;
    let mut total = 0u64;
    while cursor < latest {
        let end = cursor
            .saturating_add(MAX_LEDGER_SCAN_PER_REQUEST)
            .min(latest);
        total += ingest_range(&config, &store, &rpc, cursor, end).await?;
        store.set_cursor_ledger(end)?;
        cursor = end;
        info!(cursor, latest, total, "backfill progress");
    }

    info!(total, "backfill complete");
    Ok(())
}

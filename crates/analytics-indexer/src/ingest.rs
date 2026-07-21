//! Poll Soroban RPC `getEvents` and persist aggregator analytics events.

use {
    crate::{
        config::{IndexerConfig, DEFAULT_LOOKBACK_LEDGERS},
        events::build_invocations_from_events,
        order_events::ingest_escrow_order_events,
        parser::parse_envelope,
        store::{IndexStore, StoredInvocation},
    },
    anyhow::{Context, Result},
    dex_adapters::rpc::{
        events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
        transactions::TransactionFilterSpec,
    },
    tracing::{info, warn},
};

pub async fn run(config: IndexerConfig) -> Result<()> {
    config.ensure_parent_dir()?;
    let store = IndexStore::open(&config.db_path)?;
    let rpc = config.rpc();

    let mut cursor = resolve_start_ledger(&store, &config, &rpc).await?;
    info!(
        aggregator = %config.aggregator_contract,
        escrow = config.escrow_contract.as_deref().unwrap_or("disabled"),
        mode = %config.index_mode,
        cursor,
        db = %config.db_path,
        "analytics indexer started"
    );

    loop {
        let latest = rpc.get_latest_ledger().await.context("getLatestLedger")?.sequence;

        if cursor >= latest {
            tokio::time::sleep(std::time::Duration::from_secs(config.poll_secs)).await;
            continue;
        }

        let end = cursor.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST).min(latest);

        match ingest_range(&config, &store, &rpc, cursor, end).await {
            Ok(ingested) => {
                store.set_cursor_ledger(end)?;
                cursor = end;
                if ingested > 0 {
                    info!(ingested, cursor, "indexed invocations");
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
    let (oldest_available, latest) = rpc
        .get_events_ledger_bounds(&config.aggregator_contract)
        .await
        .context("probe getEvents ledger bounds")?;

    let mut start = if let Some(saved) = store.cursor_ledger()? {
        saved
    } else if let Some(start) = config.start_ledger {
        start
    } else {
        latest.saturating_sub(DEFAULT_LOOKBACK_LEDGERS)
    };

    if start < oldest_available {
        info!(
            requested = start,
            oldest_available, latest, "clamping indexer cursor to RPC oldest available ledger"
        );
        start = oldest_available;
    }

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
    let mut records = Vec::new();

    if config.use_events() {
        let filters = vec![EventFilterSpec {
            contract_ids: Some(vec![config.aggregator_contract.clone()]),
            topics: None,
        }];
        let events = rpc
            .get_contract_events(start_ledger, Some(end_ledger), &filters, config.page_limit)
            .await
            .with_context(|| format!("getEvents [{start_ledger}, {end_ledger})"))?;
        records.extend(build_invocations_from_events(&events)?);
    }

    if config.envelope_fallback {
        let filters = vec![TransactionFilterSpec {
            contract_ids: Some(vec![config.aggregator_contract.clone()]),
        }];
        let txs = rpc
            .get_contract_transactions(start_ledger, Some(end_ledger), &filters, config.page_limit)
            .await
            .with_context(|| format!("getTransactions [{start_ledger}, {end_ledger})"))?;

        for tx in txs {
            let parsed = match parse_envelope(&tx.envelope_xdr, &config.aggregator_contract, tx.result_xdr.as_deref()) {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    warn!(tx = %tx.tx_hash, error = %e, "failed to parse envelope");
                    continue;
                }
            };
            records.push(StoredInvocation {
                tx_hash: tx.tx_hash.clone(),
                ledger: tx.ledger,
                created_at: tx.created_at,
                status: tx.status.clone(),
                parsed,
            });
        }
    }

    let mut ingested = 0u64;
    for record in records {
        if store.insert_invocation(&record)? {
            ingested += 1;
        }
    }

    if let Some(escrow_contract) = &config.escrow_contract {
        let filters = vec![EventFilterSpec {
            contract_ids: Some(vec![escrow_contract.clone()]),
            topics: None,
        }];
        let events = rpc
            .get_contract_events(start_ledger, Some(end_ledger), &filters, config.page_limit)
            .await
            .with_context(|| format!("getEvents escrow [{start_ledger}, {end_ledger}) for {escrow_contract}"))?;

        let orders_applied = ingest_escrow_order_events(store, &events)?;
        if orders_applied > 0 {
            info!(
                orders_applied,
                start_ledger,
                end_ledger,
                escrow = %escrow_contract,
                "indexed order events"
            );
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
        let end = cursor.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST).min(latest);
        total += ingest_range(&config, &store, &rpc, cursor, end).await?;
        store.set_cursor_ledger(end)?;
        cursor = end;
        info!(cursor, latest, total, "backfill progress");
    }

    info!(total, "backfill complete");
    Ok(())
}

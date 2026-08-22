//! Poll Soroban RPC `getEvents` and persist aggregator analytics events.

use {
    crate::{
        config::{IndexerConfig, DEFAULT_LOOKBACK_LEDGERS},
        events::build_invocations_from_events,
        order_events::ingest_escrow_order_events,
        parser::{classify_failure, classify_failure_with_diagnostics, parse_envelope},
        store::{IndexStore, StoredInvocation},
    },
    anyhow::{Context, Result},
    dex_adapters::rpc::{
        events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
        transactions::{TransactionFilterSpec, DEFAULT_TX_PAGE_LIMIT},
    },
    std::collections::{HashMap, HashSet},
    tracing::{info, warn},
};

const OFFICIAL_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";
const OFFICIAL_TESTNET_RPC_URL: &str = "https://soroban-testnet.stellar.org";

#[derive(serde::Deserialize)]
struct HorizonTransaction {
    envelope_xdr: String,
    result_meta_xdr: Option<String>,
    result_xdr: Option<String>,
}

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
                // RPC retention can move forward after restarts; reclamp so we
                // do not retry forever below oldestLedger.
                if let Ok((oldest, _)) = rpc.get_events_ledger_bounds(&config.aggregator_contract).await {
                    if cursor < oldest {
                        info!(
                            requested = cursor,
                            oldest_available = oldest,
                            "reclamping indexer cursor after ingest failure"
                        );
                        store.set_cursor_ledger(oldest)?;
                        cursor = oldest;
                    }
                }
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
    let mut ingested = 0u64;

    if config.use_events() {
        let filters = vec![EventFilterSpec {
            contract_ids: Some(vec![config.aggregator_contract.clone()]),
            topics: None,
        }];
        let events = rpc
            .get_contract_events(start_ledger, Some(end_ledger), &filters, config.page_limit)
            .await
            .with_context(|| format!("getEvents [{start_ledger}, {end_ledger})"))?;
        for record in build_invocations_from_events(&events)? {
            if store.insert_invocation(&record)? {
                ingested += 1;
            } else {
                // An older envelope fallback row may already exist without
                // amount_out. Merge the event summary so successful trades
                // do not remain permanently incomplete in the public API.
                let _ = store.replace_invocation_legs(&record.tx_hash, &record.parsed)?;
            }
        }
    }

    if config.envelope_fallback {
        // Soft-fail: never block event-based cursor advance on getTransactions.
        // A hard error here previously left the cursor stuck while events were
        // already persisted for the same range.
        let filters = vec![TransactionFilterSpec {
            contract_ids: Some(vec![config.aggregator_contract.clone()]),
        }];
        match rpc
            // getTransactions is capped at 200 even when getEvents accepts
            // the larger configured page limit.
            .get_contract_transactions(start_ledger, Some(end_ledger), &filters, config.page_limit.min(200))
            .await
        {
            Ok(txs) => {
                for tx in txs {
                    // A failed Soroban transaction can have an envelope that
                    // this parser cannot decode, while resultXdr is still
                    // sufficient to classify the terminal failure. Update an
                    // event-derived row before attempting envelope parsing.
                    let failure_reason = if tx.status == "FAILED" {
                        classify_failure_with_diagnostics(tx.result_xdr.as_deref(), &tx.diagnostic_events_xdr)
                    } else {
                        None
                    };
                    if failure_reason.is_some() {
                        let _ = store.update_invocation_failure_reason(&tx.tx_hash, failure_reason.as_deref())?;
                    }
                    let parsed = match parse_envelope(&tx.envelope_xdr, &config.aggregator_contract, None) {
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
                        failure_reason,
                        parsed,
                    };
                    // Enrich event-derived actual leg amounts with envelope token
                    // and path metadata.
                    if store.insert_invocation(&record)? {
                        ingested += 1;
                    } else {
                        let _ = store
                            .update_invocation_failure_reason(&record.tx_hash, record.failure_reason.as_deref())?;
                        let _ = store.replace_invocation_legs(&record.tx_hash, &record.parsed)?;
                    }
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    start_ledger,
                    end_ledger,
                    "getTransactions envelope fallback failed; continuing with events"
                );
            }
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

/// Re-parse envelopes for stored txs and enrich leg route metadata while
/// preserving actual event-derived amounts.
pub async fn repair_leg_indices(config: IndexerConfig, created_at_from: i64) -> Result<u64> {
    config.ensure_parent_dir()?;
    let store = IndexStore::open(&config.db_path)?;
    let rpc = config.rpc();
    let official_rpc = dex_adapters::SorobanRpc::new(
        if config.network_passphrase == "Test SDF Network ; September 2015" {
            OFFICIAL_TESTNET_RPC_URL
        } else {
            OFFICIAL_MAINNET_RPC_URL
        },
        &config.network_passphrase,
    );
    let http = reqwest::Client::new();
    let txs = store.list_tx_hashes_since(created_at_from)?;
    let mut fixed = 0u64;
    for (tx_hash, _ledger) in txs {
        let own_rpc_tx = rpc.get_transaction(&tx_hash).await;
        // Prefer the self-hosted RPC. Use the official endpoint only when the
        // local node has pruned the transaction or its Soroban result metadata.
        let rpc_tx = match own_rpc_tx {
            Ok(tx) if tx.envelope_xdr.is_some() && tx.result_meta_xdr.is_some() => Ok(tx),
            own => official_rpc.get_transaction(&tx_hash).await.or(own),
        };
        let rpc_failure = rpc_tx.as_ref().err().map(ToString::to_string);
        let rpc_envelope = rpc_tx.as_ref().ok().and_then(|got| got.envelope_xdr.as_deref());
        let rpc_result_meta = rpc_tx.as_ref().ok().and_then(|got| got.result_meta_xdr.as_deref());
        if let Some(status) = rpc_tx.as_ref().ok().map(|got| got.status.as_str()) {
            let _ = store.update_invocation_status(&tx_hash, status)?;
        }
        if let Some(reason) = rpc_tx
            .as_ref()
            .ok()
            .and_then(|got| classify_failure_with_diagnostics(got.result_xdr.as_deref(), &got.diagnostic_events_xdr))
        {
            let _ = store.update_invocation_failure_reason(&tx_hash, Some(&reason))?;
        }

        let horizon_tx = if rpc_envelope.is_none() {
            match fetch_horizon_transaction(&http, config.horizon_url.as_deref(), &tx_hash).await {
                Ok(tx) => Some(tx),
                Err(e) => {
                    warn!(
                        tx = %tx_hash,
                        rpc_error = rpc_failure.as_deref().unwrap_or("missing envelopeXdr"),
                        horizon_error = %e,
                        "repair could not load transaction envelope"
                    );
                    continue;
                }
            }
        } else {
            None
        };

        let (envelope_xdr, result_meta_xdr) = match horizon_tx.as_ref() {
            Some(tx) => (tx.envelope_xdr.as_str(), tx.result_meta_xdr.as_deref()),
            None => (rpc_envelope.expect("checked above"), rpc_result_meta),
        };
        let parsed = match parse_envelope(envelope_xdr, &config.aggregator_contract, result_meta_xdr) {
            Ok(Some(p)) => p,
            Ok(None) => {
                warn!(tx = %tx_hash, "repair envelope had no aggregator invoke");
                continue;
            }
            Err(e) => {
                warn!(tx = %tx_hash, error = %e, "repair parse failed");
                continue;
            }
        };
        if store.replace_invocation_legs(&tx_hash, &parsed)? {
            fixed += 1;
            info!(
                tx = %tx_hash,
                legs = parsed.legs.len(),
                max_idx = parsed.legs.iter().map(|l| l.leg_index).max().unwrap_or(0),
                is_split = parsed.is_split,
                "repaired leg indices"
            );
        }
    }
    info!(fixed, "leg index repair complete");
    Ok(fixed)
}

/// Classify failed round trips whose result XDR or diagnostic events were not
/// available during the original ingest. This deliberately avoids reparsing
/// envelopes or legs.
pub async fn repair_failure_reasons(config: IndexerConfig) -> Result<u64> {
    config.ensure_parent_dir()?;
    let store = IndexStore::open(&config.db_path)?;
    let rpc = config.rpc();
    let official_rpc = dex_adapters::SorobanRpc::new(
        if config.network_passphrase == "Test SDF Network ; September 2015" {
            OFFICIAL_TESTNET_RPC_URL
        } else {
            OFFICIAL_MAINNET_RPC_URL
        },
        &config.network_passphrase,
    );
    let http = reqwest::Client::new();
    let txs = store.list_unclassified_failed_transactions()?;
    let filters = vec![TransactionFilterSpec {
        contract_ids: Some(vec![config.aggregator_contract.clone()]),
    }];
    let mut fixed = 0u64;
    let mut diagnostic_txs = HashMap::new();
    let mut offset = 0;
    while offset < txs.len() {
        let chunk_start = (txs[offset].1 / MAX_LEDGER_SCAN_PER_REQUEST) * MAX_LEDGER_SCAN_PER_REQUEST;
        let chunk_end = chunk_start.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST);
        let mut chunk_hashes = HashSet::new();
        while offset < txs.len() && txs[offset].1 < chunk_end {
            chunk_hashes.insert(txs[offset].0.clone());
            offset += 1;
        }

        let own_items = rpc
            .get_contract_transactions(chunk_start.max(1), Some(chunk_end), &filters, DEFAULT_TX_PAGE_LIMIT)
            .await
            .unwrap_or_default();
        for tx in own_items {
            if chunk_hashes.contains(&tx.tx_hash) {
                diagnostic_txs.insert(tx.tx_hash.clone(), tx);
            }
        }

        let missing = chunk_hashes
            .iter()
            .filter(|hash| !diagnostic_txs.contains_key(*hash))
            .count();
        if missing > 0 {
            if let Ok(items) = official_rpc
                .get_contract_transactions(chunk_start.max(1), Some(chunk_end), &filters, DEFAULT_TX_PAGE_LIMIT)
                .await
            {
                for tx in items {
                    if chunk_hashes.contains(&tx.tx_hash) {
                        diagnostic_txs.insert(tx.tx_hash.clone(), tx);
                    }
                }
            }
        }
    }

    for (tx_hash, _ledger) in txs {
        if let Some(tx) = diagnostic_txs.get(&tx_hash) {
            if let Some(reason) = classify_failure_with_diagnostics(tx.result_xdr.as_deref(), &tx.diagnostic_events_xdr)
            {
                if store.refine_invocation_failure_reason(&tx_hash, &reason)? {
                    fixed += 1;
                    continue;
                }
            }
        }

        let own = rpc.get_transaction(&tx_hash).await;
        let rpc_tx = match own {
            // A self-hosted node may retain resultXdr after pruning the
            // diagnostic events. Prefer it only when both are present so the
            // official RPC can provide the missing failure classification.
            Ok(tx) if tx.result_xdr.is_some() && !tx.diagnostic_events_xdr.is_empty() => Ok(tx),
            own => official_rpc.get_transaction(&tx_hash).await.or(own),
        };
        let reason = rpc_tx
            .as_ref()
            .ok()
            .and_then(|tx| classify_failure_with_diagnostics(tx.result_xdr.as_deref(), &tx.diagnostic_events_xdr));
        let reason = match reason {
            Some(reason) => Some(reason),
            None => match fetch_horizon_transaction(&http, config.horizon_url.as_deref(), &tx_hash).await {
                Ok(tx) => classify_failure(tx.result_xdr.as_deref()),
                Err(_) => None,
            },
        };
        let Some(reason) = reason else {
            continue;
        };
        if store.refine_invocation_failure_reason(&tx_hash, &reason)? {
            fixed += 1;
        }
    }
    info!(fixed, "failure reason repair complete");
    Ok(fixed)
}

async fn fetch_horizon_transaction(
    http: &reqwest::Client,
    horizon_url: Option<&str>,
    tx_hash: &str,
) -> Result<HorizonTransaction> {
    let horizon_url = horizon_url.context("dex.horizon_url is not configured")?;
    let url = format!("{}/transactions/{tx_hash}", horizon_url.trim_end_matches('/'));
    http.get(url)
        .send()
        .await
        .context("request Horizon transaction")?
        .error_for_status()
        .context("Horizon transaction response")?
        .json()
        .await
        .context("decode Horizon transaction")
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

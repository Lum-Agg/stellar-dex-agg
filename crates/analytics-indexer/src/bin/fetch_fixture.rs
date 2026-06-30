//! One-shot helper: scan mainnet and save the first parsed aggregator swap envelope.

use analytics_indexer::{
    config::DEFAULT_AGGREGATOR_CONTRACT,
    parser::parse_envelope,
};
use dex_adapters::rpc::transactions::{TransactionFilterSpec, DEFAULT_TX_PAGE_LIMIT, MAX_LEDGER_SCAN_PER_REQUEST};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc = dex_adapters::SorobanRpc::mainnet();
    let latest = rpc.get_latest_ledger().await?.sequence;
    let start = std::env::var("INDEXER_START_LEDGER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(latest.saturating_sub(50_000));

    let filters = vec![TransactionFilterSpec {
        contract_ids: Some(vec![DEFAULT_AGGREGATOR_CONTRACT.into()]),
    }];

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swap_envelope.b64");
    let mut cursor = start;
    while cursor < latest {
        let end = cursor.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST).min(latest);
        let txs = rpc
            .get_contract_transactions(cursor, Some(end), &filters, DEFAULT_TX_PAGE_LIMIT)
            .await?;
        for tx in txs {
            if let Ok(Some(parsed)) = parse_envelope(
                &tx.envelope_xdr,
                DEFAULT_AGGREGATOR_CONTRACT,
                tx.result_xdr.as_deref(),
            ) {
                std::fs::write(&out, &tx.envelope_xdr)?;
                println!(
                    "saved {} ({}, {} legs) -> {}",
                    tx.tx_hash,
                    parsed.function_name,
                    parsed.legs.len(),
                    out.display()
                );
                return Ok(());
            }
        }
        cursor = end;
        eprintln!("scanned through ledger {cursor}");
    }

    anyhow::bail!("no aggregator swap/round_trip invocation found in range [{start}, {latest})");
}

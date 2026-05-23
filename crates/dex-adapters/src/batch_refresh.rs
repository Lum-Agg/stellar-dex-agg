//! Batch reserves refresh using getLedgerEntries.
//!
//! Instead of calling get_reserves() on each pool individually (1 RPC per pool),
//! we read the contract instance data for all pools in a single getLedgerEntries call.
//!
//! Soroswap pairs store reserves in instance storage:
//!   DataKey::Reserve0 = U32(2) → I128
//!   DataKey::Reserve1 = U32(3) → I128
//!
//! The instance ledger entry contains ALL instance storage data for a contract,
//! so one getLedgerEntries call with N contract instance keys gives us N pools' reserves.

use crate::rpc::SorobanRpc;
use anyhow::{anyhow, Result};
use serde_json::json;
use stellar_xdr::curr::{self as xdr, Limits, ReadXdr, WriteXdr};
use tracing::debug;

/// Maximum keys per getLedgerEntries call (Stellar RPC limit)
const MAX_KEYS_PER_CALL: usize = 200;

/// Batch-read contract instance data for multiple Soroswap pairs.
/// Returns a map of pool_address -> (reserve0, reserve1).
pub async fn batch_refresh_soroswap_reserves(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
) -> Result<Vec<(String, Option<(u128, u128)>)>> {
    let mut all_results = Vec::new();

    for chunk in pool_addresses.chunks(MAX_KEYS_PER_CALL) {
        let results = fetch_instance_data_batch(rpc, chunk).await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

/// Same as [`batch_refresh_soroswap_reserves`] but runs up to `max_in_flight` ledger batches concurrently.
pub async fn batch_refresh_soroswap_reserves_parallel(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
    max_in_flight: usize,
) -> Result<Vec<(String, Option<(u128, u128)>)>> {
    if pool_addresses.is_empty() {
        return Ok(Vec::new());
    }
    let concurrency = max_in_flight.max(1);
    let chunks: Vec<Vec<String>> = pool_addresses
        .chunks(MAX_KEYS_PER_CALL)
        .map(|c| c.to_vec())
        .collect();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut tasks = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let sem = semaphore.clone();
        let rpc_url = rpc.url().to_string();
        let passphrase = rpc.network_passphrase().to_string();
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore");
            let rpc = SorobanRpc::new(&rpc_url, &passphrase);
            fetch_instance_data_batch(&rpc, &chunk).await
        }));
    }
    let mut all_results = Vec::with_capacity(pool_addresses.len());
    for task in tasks {
        all_results.extend(task.await??);
    }
    Ok(all_results)
}

/// Fetch contract instance data for a batch of contracts.
async fn fetch_instance_data_batch(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
) -> Result<Vec<(String, Option<(u128, u128)>)>> {
    // Build ledger keys for contract instance data
    let mut key_xdrs: Vec<String> = Vec::new();

    for addr in pool_addresses {
        let contract_hash = stellar_strkey::Contract::from_string(addr)
            .map_err(|e| anyhow!("Invalid contract address {}: {:?}", addr, e))?
            .0;

        let ledger_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
            contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
            key: xdr::ScVal::LedgerKeyContractInstance,
            durability: xdr::ContractDataDurability::Persistent,
        });

        let key_b64 = ledger_key
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR encode error: {:?}", e))?;

        key_xdrs.push(key_b64);
    }

    // Call getLedgerEntries
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLedgerEntries",
        "params": {
            "keys": key_xdrs
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = client
        .post(rpc.url())
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("RPC request failed: {}", e))?;

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("RPC response parse failed: {}", e))?;

    if let Some(error) = resp_json.get("error") {
        return Err(anyhow!("RPC error: {}", error));
    }

    let entries = resp_json
        .get("result")
        .and_then(|r| r.get("entries"))
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    println!("[batch_refresh] RPC returned {} entries", entries.len());

    // Parse results
    let mut results: Vec<(String, Option<(u128, u128)>)> = pool_addresses
        .iter()
        .map(|addr| (addr.clone(), None))
        .collect();

    for (i, entry_val) in entries.iter().enumerate() {
        if i >= pool_addresses.len() {
            break;
        }

        let xdr_b64 = match entry_val.get("xdr").and_then(|x| x.as_str()) {
            Some(x) => x,
            None => continue,
        };

        match parse_instance_reserves(xdr_b64) {
            Ok(Some((r0, r1))) => {
                results[i].1 = Some((r0, r1));
            }
            Ok(None) => {}
            Err(e) => {
                debug!("Failed to parse reserves for {}: {}", pool_addresses[i], e);
            }
        }
    }

    Ok(results)
}

/// Parse a contract instance ledger entry to extract Reserve0 and Reserve1.
///
/// The instance storage is a map of ScVal keys to ScVal values.
/// We look for U32(2) -> I128 (Reserve0) and U32(3) -> I128 (Reserve1).
fn parse_instance_reserves(xdr_b64: &str) -> Result<Option<(u128, u128)>> {
    // The RPC returns LedgerEntryData XDR (not full LedgerEntry)
    // Try decoding as LedgerEntryData first, then fall back to LedgerEntry
    let data = if let Ok(entry) = xdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none()) {
        entry.data
    } else if let Ok(data) = xdr::LedgerEntryData::from_xdr_base64(xdr_b64, Limits::none()) {
        data
    } else if let Ok(cd) = xdr::ContractDataEntry::from_xdr_base64(xdr_b64, Limits::none()) {
        xdr::LedgerEntryData::ContractData(cd)
    } else {
        return Err(anyhow!("Cannot decode XDR as any known type"));
    };

    let contract_data = match &data {
        xdr::LedgerEntryData::ContractData(cd) => cd,
        other => {
            println!(
                "[parse] Not ContractData: {:?}",
                std::mem::discriminant(other)
            );
            return Ok(None);
        }
    };

    let instance = match &contract_data.val {
        xdr::ScVal::ContractInstance(inst) => inst,
        other => {
            println!(
                "[parse] val is not ContractInstance: {:?}",
                std::mem::discriminant(other)
            );
            return Ok(None);
        }
    };

    let storage = match &instance.storage {
        Some(map) => map,
        None => {
            println!("[parse] No storage in instance");
            return Ok(None);
        }
    };

    let mut reserve0: Option<u128> = None;
    let mut reserve1: Option<u128> = None;

    for entry in storage.0.iter() {
        match &entry.key {
            xdr::ScVal::U32(2) => {
                reserve0 = extract_i128_as_u128(&entry.val);
            }
            xdr::ScVal::U32(3) => {
                reserve1 = extract_i128_as_u128(&entry.val);
            }
            _ => {}
        }
    }

    match (reserve0, reserve1) {
        (Some(r0), Some(r1)) => Ok(Some((r0, r1))),
        _ => Ok(None),
    }
}

/// Extract a u128 value from an ScVal (handles both I128 and U128).
fn extract_i128_as_u128(val: &xdr::ScVal) -> Option<u128> {
    match val {
        xdr::ScVal::I128(parts) => {
            let v = ((parts.hi as i128) << 64) | (parts.lo as u64 as i128);
            Some(v as u128)
        }
        xdr::ScVal::U128(parts) => Some(((parts.hi as u128) << 64) | (parts.lo as u128)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // requires network
    async fn test_batch_refresh_soroswap() {
        let rpc = SorobanRpc::new(
            "http://178.63.81.216:8003",
            "Public Global Stellar Network ; September 2015",
        );

        // First Soroswap pair (from our earlier tests)
        let pools = vec!["CB46LMGJC7SYSH4C7SBNLV635OX5BSNQDGRR32NRXAV7N2AVNZMQUJ3A".to_string()];

        let results = batch_refresh_soroswap_reserves(&rpc, &pools).await.unwrap();

        println!("Results: {:?}", results);
        assert_eq!(results.len(), 1);

        if results[0].1.is_none() {
            // Debug: try to see what the raw response looks like
            println!("Reserves are None - checking raw response...");
            // Try reading the instance directly
            let contract_hash = stellar_strkey::Contract::from_string(&pools[0]).unwrap().0;
            let ledger_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
                contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
                key: xdr::ScVal::LedgerKeyContractInstance,
                durability: xdr::ContractDataDurability::Persistent,
            });
            let key_b64 = ledger_key.to_xdr_base64(Limits::none()).unwrap();
            println!("Key XDR: {}", key_b64);

            // Also try with simulate to compare
            let sim_result = rpc.call_no_args(&pools[0], "get_reserves").await;
            println!("Simulate get_reserves: {:?}", sim_result);
        }

        assert!(results[0].1.is_some(), "Should have reserves");

        let (r0, r1) = results[0].1.unwrap();
        println!("Reserve0: {}, Reserve1: {}", r0, r1);
        assert!(r0 > 0 && r1 > 0);
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_batch_refresh_multiple() {
        let rpc = SorobanRpc::new(
            "http://178.63.81.216:8003",
            "Public Global Stellar Network ; September 2015",
        );

        // Multiple Soroswap pairs
        let pools = vec![
            "CB46LMGJC7SYSH4C7SBNLV635OX5BSNQDGRR32NRXAV7N2AVNZMQUJ3A".to_string(),
            "CBJ3WO7M3H7EI7ATEBYHLZBJCW4OXHU3FRG7LK6ZTRHLKKLFW5NHY4Q6".to_string(),
            "CACXB6KH5DQVQKQGXKHF2M5TEKFY5KIDCKSEXZYJ27Z5465V2SSALCBW".to_string(),
        ];

        let results = batch_refresh_soroswap_reserves(&rpc, &pools).await.unwrap();

        println!("Batch results ({} pools):", results.len());
        for (addr, reserves) in &results {
            println!("  {} -> {:?}", &addr[..10], reserves);
        }

        assert_eq!(results.len(), 3);
    }
}

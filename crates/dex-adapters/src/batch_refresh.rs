//! Batch reserves refresh using getLedgerEntries.
//!
//! Instead of calling get_reserves() on each pool individually (1 RPC per
//! pool), we read the contract instance data for all pools in a single
//! getLedgerEntries call.
//!
//! Soroswap pairs store reserves in instance storage:
//!   DataKey::Reserve0 = U32(2) → I128
//!   DataKey::Reserve1 = U32(3) → I128
//!
//! Aquarius volatile pools (liquidity_pool):
//!   DataKey::ReserveA = U32(2), DataKey::ReserveB = U32(3)
//!
//! Aquarius stableswap pools (liquidity_pool_stableswap):
//!   DataKey::Reserves = U32(2) → Vec<u128>

use {
    crate::rpc::SorobanRpc,
    anyhow::{anyhow, Result},
    serde_json::json,
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr, WriteXdr},
    tracing::debug,
};

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
        let results = fetch_soroswap_reserves_batch(rpc, chunk).await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

/// Same as [`batch_refresh_soroswap_reserves`] but runs up to `max_in_flight`
/// ledger batches concurrently.
pub async fn batch_refresh_soroswap_reserves_parallel(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
    max_in_flight: usize,
) -> Result<Vec<(String, Option<(u128, u128)>)>> {
    if pool_addresses.is_empty() {
        return Ok(Vec::new());
    }
    let concurrency = max_in_flight.max(1);
    let chunks: Vec<Vec<String>> = pool_addresses.chunks(MAX_KEYS_PER_CALL).map(|c| c.to_vec()).collect();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut tasks = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let sem = semaphore.clone();
        let rpc_url = rpc.url().to_string();
        let passphrase = rpc.network_passphrase().to_string();
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore");
            let rpc = SorobanRpc::new(&rpc_url, &passphrase);
            fetch_soroswap_reserves_batch(&rpc, &chunk).await
        }));
    }
    let mut all_results = Vec::with_capacity(pool_addresses.len());
    for task in tasks {
        all_results.extend(task.await??);
    }
    Ok(all_results)
}

/// Batch-read Aquarius pool reserves (volatile + stableswap) from contract
/// instance storage.
pub async fn batch_refresh_aquarius_reserves_parallel(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
    max_in_flight: usize,
) -> Result<Vec<(String, Option<Vec<u128>>)>> {
    if pool_addresses.is_empty() {
        return Ok(Vec::new());
    }
    let concurrency = max_in_flight.max(1);
    let chunks: Vec<Vec<String>> = pool_addresses.chunks(MAX_KEYS_PER_CALL).map(|c| c.to_vec()).collect();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut tasks = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let sem = semaphore.clone();
        let rpc_url = rpc.url().to_string();
        let passphrase = rpc.network_passphrase().to_string();
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore");
            let rpc = SorobanRpc::new(&rpc_url, &passphrase);
            fetch_aquarius_reserves_batch(&rpc, &chunk).await
        }));
    }
    let mut all_results = Vec::with_capacity(pool_addresses.len());
    for task in tasks {
        all_results.extend(task.await??);
    }
    Ok(all_results)
}

/// Fetch contract instance data for a batch of Soroswap contracts.
async fn fetch_soroswap_reserves_batch(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
) -> Result<Vec<(String, Option<(u128, u128)>)>> {
    let entries = fetch_instance_ledger_xdrs(rpc, pool_addresses).await?;
    let mut results: Vec<(String, Option<(u128, u128)>)> =
        pool_addresses.iter().map(|addr| (addr.clone(), None)).collect();

    for (i, xdr_b64) in entries.into_iter().enumerate() {
        let Some(xdr_b64) = xdr_b64 else {
            continue;
        };
        match parse_soroswap_instance_reserves(&xdr_b64) {
            Ok(Some((r0, r1))) => {
                results[i].1 = Some((r0, r1));
            }
            Ok(None) => {}
            Err(e) => {
                debug!("Failed to parse Soroswap reserves for {}: {}", pool_addresses[i], e);
            }
        }
    }

    Ok(results)
}

/// Fetch contract instance data for a batch of Aquarius pool contracts.
async fn fetch_aquarius_reserves_batch(
    rpc: &SorobanRpc,
    pool_addresses: &[String],
) -> Result<Vec<(String, Option<Vec<u128>>)>> {
    let entries = fetch_instance_ledger_xdrs(rpc, pool_addresses).await?;
    let mut results: Vec<(String, Option<Vec<u128>>)> =
        pool_addresses.iter().map(|addr| (addr.clone(), None)).collect();

    for (i, xdr_b64) in entries.into_iter().enumerate() {
        let Some(xdr_b64) = xdr_b64 else {
            continue;
        };
        match parse_aquarius_instance_reserves(&xdr_b64) {
            Ok(Some(reserves)) => {
                results[i].1 = Some(reserves);
            }
            Ok(None) => {}
            Err(e) => {
                debug!("Failed to parse Aquarius reserves for {}: {}", pool_addresses[i], e);
            }
        }
    }

    Ok(results)
}

async fn fetch_instance_ledger_xdrs(rpc: &SorobanRpc, pool_addresses: &[String]) -> Result<Vec<Option<String>>> {
    let key_xdrs = build_instance_ledger_key_xdrs(pool_addresses)?;

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

    debug!(
        pools = pool_addresses.len(),
        entries = entries.len(),
        "getLedgerEntries batch"
    );

    let mut out = vec![None; pool_addresses.len()];
    for (i, entry_val) in entries.iter().enumerate() {
        if i >= pool_addresses.len() {
            break;
        }
        if let Some(x) = entry_val.get("xdr").and_then(|x| x.as_str()) {
            out[i] = Some(x.to_string());
        }
    }
    Ok(out)
}

fn build_instance_ledger_key_xdrs(pool_addresses: &[String]) -> Result<Vec<String>> {
    let mut key_xdrs = Vec::with_capacity(pool_addresses.len());
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
    Ok(key_xdrs)
}

fn contract_instance_storage(xdr_b64: &str) -> Result<Option<xdr::ScMap>> {
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
        _ => return Ok(None),
    };

    let instance = match &contract_data.val {
        xdr::ScVal::ContractInstance(inst) => inst,
        _ => return Ok(None),
    };

    Ok(instance.storage.clone())
}

/// Parse Soroswap instance storage: U32(2)=Reserve0, U32(3)=Reserve1.
fn parse_soroswap_instance_reserves(xdr_b64: &str) -> Result<Option<(u128, u128)>> {
    let Some(storage) = contract_instance_storage(xdr_b64)? else {
        return Ok(None);
    };

    let mut reserve0: Option<u128> = None;
    let mut reserve1: Option<u128> = None;

    for entry in storage.0.iter() {
        match &entry.key {
            xdr::ScVal::U32(2) => {
                reserve0 = scval_to_u128(&entry.val);
            }
            xdr::ScVal::U32(3) => {
                reserve1 = scval_to_u128(&entry.val);
            }
            _ => {}
        }
    }

    match (reserve0, reserve1) {
        (Some(r0), Some(r1)) => Ok(Some((r0, r1))),
        _ => Ok(None),
    }
}

/// Parse Aquarius instance storage.
///
/// Volatile: U32(2)=ReserveA, U32(3)=ReserveB.
/// Stableswap: U32(2)=Reserves (Vec<u128>).
fn parse_aquarius_instance_reserves(xdr_b64: &str) -> Result<Option<Vec<u128>>> {
    let Some(storage) = contract_instance_storage(xdr_b64)? else {
        return Ok(None);
    };

    let mut reserve_a: Option<u128> = None;
    let mut reserve_b: Option<u128> = None;
    let mut stable_reserves: Option<Vec<u128>> = None;

    for entry in storage.0.iter() {
        match &entry.key {
            xdr::ScVal::U32(2) => {
                if let Some(vec) = scval_to_u128_vec(&entry.val) {
                    stable_reserves = Some(vec);
                } else {
                    reserve_a = scval_to_u128(&entry.val);
                }
            }
            xdr::ScVal::U32(3) => {
                reserve_b = scval_to_u128(&entry.val);
            }
            _ => {}
        }
    }

    if let Some(reserves) = stable_reserves {
        if reserves.is_empty() {
            Ok(None)
        } else {
            Ok(Some(reserves))
        }
    } else if let (Some(a), Some(b)) = (reserve_a, reserve_b) {
        Ok(Some(vec![a, b]))
    } else {
        Ok(None)
    }
}

fn scval_to_u128(val: &xdr::ScVal) -> Option<u128> {
    match val {
        xdr::ScVal::I128(parts) => {
            let v = ((parts.hi as i128) << 64) | (parts.lo as u64 as i128);
            Some(v as u128)
        }
        xdr::ScVal::U128(parts) => Some(((parts.hi as u128) << 64) | (parts.lo as u128)),
        _ => None,
    }
}

fn scval_to_u128_vec(val: &xdr::ScVal) -> Option<Vec<u128>> {
    let xdr::ScVal::Vec(Some(vec)) = val else {
        return None;
    };
    let mut out = Vec::with_capacity(vec.0.len());
    for item in vec.0.iter() {
        out.push(scval_to_u128(item)?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // requires network
    async fn test_batch_refresh_soroswap() {
        let rpc = SorobanRpc::new(
            "http://88.198.16.144:8003",
            "Public Global Stellar Network ; September 2015",
        );

        let pools = vec!["CB46LMGJC7SYSH4C7SBNLV635OX5BSNQDGRR32NRXAV7N2AVNZMQUJ3A".to_string()];

        let results = batch_refresh_soroswap_reserves(&rpc, &pools).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_some(), "Should have reserves");

        let (r0, r1) = results[0].1.unwrap();
        assert!(r0 > 0 && r1 > 0);
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_batch_refresh_aquarius() {
        let rpc = SorobanRpc::new(
            "http://88.198.16.144:8003",
            "Public Global Stellar Network ; September 2015",
        );

        // USDC/CBVDRT stable pool + XLM/CBVDRT volatile pool from production debugging.
        let pools = vec![
            "CBRXOYK7YTO7FNO4R7ZHOUDIPNB3FE2JXSDGB6AOHLWHMQULXQ7PX6Y".to_string(),
            "CDYLKM3YI3LFD2AFUXILLBO2ABDHYKDD3RXTLOUVW2C7BXEQPZ65IMFX".to_string(),
        ];

        let results = batch_refresh_aquarius_reserves_parallel(&rpc, &pools, 2).await.unwrap();

        assert_eq!(results.len(), 2);
        for (addr, reserves) in &results {
            let reserves = reserves
                .as_ref()
                .unwrap_or_else(|| panic!("missing reserves for {addr}"));
            assert!(!reserves.is_empty());
            assert!(reserves.iter().any(|&r| r > 0));
        }
    }
}

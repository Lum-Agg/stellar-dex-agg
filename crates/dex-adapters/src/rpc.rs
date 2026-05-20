//! Soroban RPC client wrapper for DEX adapter interactions.
//!
//! Provides contract simulation (read-only calls) and ledger entry queries
//! needed by all Soroban DEX adapters.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use stellar_xdr::curr as xdr;
use tracing::{debug, warn};

/// Lightweight Soroban RPC client focused on what DEX adapters need:
/// - simulateTransaction (for read-only contract calls)
/// - getLedgerEntries (for reading pool state)
pub struct SorobanRpc {
    url: String,
    client: Client,
    network_passphrase: String,
}

impl SorobanRpc {
    pub fn new(url: &str, network_passphrase: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            url: url.to_string(),
            client,
            network_passphrase: network_passphrase.to_string(),
        }
    }

    /// Mainnet default
    pub fn mainnet() -> Self {
        Self::new(
            "https://soroban-rpc.mainnet.stellar.gateway.fm",
            "Public Global Stellar Network ; September 2015",
        )
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Simulate a contract call (read-only, no submission).
    /// Returns the ScVal result.
    pub async fn simulate_call(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<xdr::ScVal>,
    ) -> Result<xdr::ScVal> {
        use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

        // Build a dummy transaction for simulation
        let contract_hash = stellar_strkey::Contract::from_string(contract_address)
            .map_err(|e| anyhow!("Invalid contract address: {:?}", e))?
            .0;

        let invoke_args = xdr::InvokeContractArgs {
            contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
            function_name: function_name
                .try_into()
                .map_err(|_| anyhow!("Invalid function name"))?,
            args: args.try_into().map_err(|_| anyhow!("Too many args"))?,
        };

        let host_function = xdr::HostFunction::InvokeContract(invoke_args);

        let op = xdr::Operation {
            source_account: None,
            body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
                host_function,
                auth: xdr::VecM::default(),
            }),
        };

        // Dummy source account (zero address)
        let source_account = xdr::MuxedAccount::Ed25519(xdr::Uint256([0u8; 32]));

        let tx = xdr::Transaction {
            source_account,
            fee: 100,
            seq_num: xdr::SequenceNumber(0),
            cond: xdr::Preconditions::None,
            memo: xdr::Memo::None,
            operations: vec![op].try_into().map_err(|_| anyhow!("ops error"))?,
            ext: xdr::TransactionExt::V0,
        };

        let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
            tx,
            signatures: xdr::VecM::default(),
        });

        let tx_xdr = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR encode error: {:?}", e))?;

        // Call simulateTransaction RPC
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": {
                "transaction": tx_xdr
            }
        });

        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("RPC request failed: {}", e))?;

        let resp_json: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("RPC response parse failed: {}", e))?;

        // Check for error
        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {}", error));
        }

        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result in RPC response"))?;

        // Check simulation error
        if let Some(error) = result.get("error") {
            return Err(anyhow!("Simulation error: {}", error));
        }

        // Extract return value from results[0].xdr
        let results = result
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow!("No results array"))?;

        if results.is_empty() {
            return Err(anyhow!("Empty results"));
        }

        let xdr_b64 = results[0]
            .get("xdr")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("No xdr in result"))?;

        let scval = xdr::ScVal::from_xdr_base64(xdr_b64, Limits::none())
            .map_err(|e| anyhow!("ScVal decode error: {:?}", e))?;

        Ok(scval)
    }

    /// Convenience: call a contract function with no arguments.
    pub async fn call_no_args(&self, contract: &str, function: &str) -> Result<xdr::ScVal> {
        self.simulate_call(contract, function, vec![]).await
    }

    /// Get ledger entries by key.
    pub async fn get_ledger_entries(
        &self,
        keys: Vec<xdr::LedgerKey>,
    ) -> Result<Vec<LedgerEntryResult>> {
        use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

        let key_xdrs: Vec<String> = keys
            .iter()
            .map(|k| k.to_xdr_base64(Limits::none()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Key XDR encode error: {:?}", e))?;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": {
                "keys": key_xdrs
            }
        });

        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("RPC request failed: {}", e))?;

        let resp_json: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("RPC response parse failed: {}", e))?;

        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {}", error));
        }

        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result"))?;

        let empty_vec = vec![];
        let entries = result
            .get("entries")
            .and_then(|e| e.as_array())
            .unwrap_or(&empty_vec);

        let mut results = Vec::new();
        for entry in entries {
            let xdr_b64 = entry
                .get("xdr")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("No xdr in entry"))?;

            let ledger_entry = xdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none())
                .map_err(|e| anyhow!("LedgerEntry decode error: {:?}", e))?;

            results.push(LedgerEntryResult {
                entry: ledger_entry,
            });
        }

        Ok(results)
    }

    pub fn network_passphrase(&self) -> &str {
        &self.network_passphrase
    }
}

#[derive(Debug)]
pub struct LedgerEntryResult {
    pub entry: xdr::LedgerEntry,
}

// ===== ScVal extraction helpers =====

/// Extract u32 from ScVal
pub fn scval_to_u32(val: &xdr::ScVal) -> Result<u32> {
    match val {
        xdr::ScVal::U32(v) => Ok(*v),
        _ => Err(anyhow!(
            "Expected U32, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

/// Extract u128 from ScVal
pub fn scval_to_u128(val: &xdr::ScVal) -> Result<u128> {
    match val {
        xdr::ScVal::U128(parts) => Ok(((parts.hi as u128) << 64) | (parts.lo as u128)),
        _ => Err(anyhow!(
            "Expected U128, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

/// Extract i128 from ScVal
pub fn scval_to_i128(val: &xdr::ScVal) -> Result<i128> {
    match val {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | (parts.lo as u64 as i128)),
        _ => Err(anyhow!(
            "Expected I128, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

/// Extract Address string from ScVal
pub fn scval_to_address(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash)))) => {
            Ok(format!("{}", stellar_strkey::Contract(*hash)))
        }
        xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
            xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(key)),
        ))) => Ok(format!("{}", stellar_strkey::ed25519::PublicKey(*key))),
        _ => Err(anyhow!(
            "Expected Address, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

/// Extract String from ScVal
pub fn scval_to_string(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::String(s) => Ok(s.to_string()),
        xdr::ScVal::Symbol(s) => Ok(s.to_string()),
        _ => Err(anyhow!("Expected String/Symbol")),
    }
}

/// Get a field from a ScMap by symbol key
pub fn get_map_field<'a>(map: &'a xdr::ScMap, key: &str) -> Option<&'a xdr::ScVal> {
    map.0.iter().find_map(|entry| match &entry.key {
        xdr::ScVal::Symbol(s) => {
            if s.to_string() == key {
                Some(&entry.val)
            } else {
                None
            }
        }
        _ => None,
    })
}

//! Soroban RPC client wrapper for DEX adapter interactions.
//!
//! Provides contract simulation (read-only calls), ledger entry queries,
//! and the `events` / `transactions` submodules.

pub mod events;
pub mod transactions;

use {
    anyhow::{anyhow, Result},
    reqwest::Client,
    serde_json::{json, Value},
    stellar_xdr::curr as xdr,
};

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

    async fn post_json_with_retry(&self, body: Value) -> Result<Value> {
        const MAX_ATTEMPTS: usize = 5;
        let mut last_err = anyhow!("RPC request not attempted");

        for attempt in 1..=MAX_ATTEMPTS {
            let resp = match self.client.post(&self.url).json(&body).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    last_err = anyhow!("RPC request failed: {}", e);
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64)).await;
                    }
                    continue;
                }
            };

            let text = match resp.text().await {
                Ok(text) => text,
                Err(e) => {
                    last_err = anyhow!("RPC response read failed: {}", e);
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64)).await;
                    }
                    continue;
                }
            };

            match serde_json::from_str::<Value>(&text) {
                Ok(json) => return Ok(json),
                Err(e) => {
                    last_err = anyhow!("RPC response parse failed: {}", e);
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64)).await;
                    }
                }
            }
        }

        Err(last_err)
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
            function_name: function_name.try_into().map_err(|_| anyhow!("Invalid function name"))?,
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

        let resp_json = self.post_json_with_retry(body).await?;

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

        let scval =
            xdr::ScVal::from_xdr_base64(xdr_b64, Limits::none()).map_err(|e| anyhow!("ScVal decode error: {:?}", e))?;

        Ok(scval)
    }

    /// Convenience: call a contract function with no arguments.
    pub async fn call_no_args(&self, contract: &str, function: &str) -> Result<xdr::ScVal> {
        self.simulate_call(contract, function, vec![]).await
    }

    /// Get ledger entries by key.
    pub async fn get_ledger_entries(&self, keys: Vec<xdr::LedgerKey>) -> Result<Vec<LedgerEntryResult>> {
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

        let resp_json = self.post_json_with_retry(body).await?;

        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {}", error));
        }

        let result = resp_json.get("result").ok_or_else(|| anyhow!("No result"))?;

        let empty_vec = vec![];
        let entries = result.get("entries").and_then(|e| e.as_array()).unwrap_or(&empty_vec);

        let mut results = Vec::new();
        for entry in entries {
            let xdr_b64 = entry
                .get("xdr")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("No xdr in entry"))?;

            let ledger_entry = xdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none())
                .map_err(|e| anyhow!("LedgerEntry decode error: {:?}", e))?;

            results.push(LedgerEntryResult { entry: ledger_entry });
        }

        Ok(results)
    }

    pub fn network_passphrase(&self) -> &str {
        &self.network_passphrase
    }

    /// Account sequence from `getLedgerEntries` (Account ledger key).
    ///
    /// Some RPC nodes return `LedgerEntry`, others `LedgerEntryData` / bare
    /// `AccountEntry` in the `xdr` field — try all shapes (same as arb
    /// prepare).
    pub async fn get_account_sequence(&self, public_key: &str) -> Result<i64> {
        use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

        let pk = stellar_strkey::ed25519::PublicKey::from_string(public_key)
            .map_err(|e| anyhow!("Invalid public key: {:?}", e))?;
        let account_id = xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(pk.0)));
        let key = xdr::LedgerKey::Account(xdr::LedgerKeyAccount { account_id });
        let key_b64 = key
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("encode account ledger key: {:?}", e))?;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": [key_b64] }
        });
        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("getLedgerEntries RPC error: {}", error));
        }

        let xdr_b64 = resp
            .pointer("/result/entries/0/xdr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("account not found on ledger (fund it first): {}", public_key))?;

        if let Ok(entry) = xdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none()) {
            if let xdr::LedgerEntryData::Account(data) = entry.data {
                return Ok(data.seq_num.0);
            }
        }
        if let Ok(data) = xdr::LedgerEntryData::from_xdr_base64(xdr_b64, Limits::none()) {
            if let xdr::LedgerEntryData::Account(data) = data {
                return Ok(data.seq_num.0);
            }
        }
        if let Ok(data) = xdr::AccountEntry::from_xdr_base64(xdr_b64, Limits::none()) {
            return Ok(data.seq_num.0);
        }
        Err(anyhow!("cannot decode account entry from ledger XDR"))
    }

    /// Submit a signed transaction envelope XDR via `sendTransaction`.
    pub async fn send_transaction(&self, signed_tx_xdr: &str) -> Result<SendTransactionResult> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": {
                "transaction": signed_tx_xdr
            }
        });

        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("sendTransaction RPC error: {}", error));
        }

        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("sendTransaction missing result"))?;

        Ok(SendTransactionResult {
            status: result
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            hash: result
                .get("hash")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            error_result_xdr: result
                .get("errorResultXdr")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Poll `getTransaction` for final status (`SUCCESS` / `FAILED` / …).
    pub async fn get_transaction(&self, hash: &str) -> Result<GetTransactionResult> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": {
                "hash": hash
            }
        });

        let resp = self.post_json_with_retry(body).await?;
        if let Some(error) = resp.get("error") {
            return Err(anyhow!("getTransaction RPC error: {}", error));
        }

        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("getTransaction missing result"))?;

        Ok(GetTransactionResult {
            status: result
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            envelope_xdr: result
                .get("envelopeXdr")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
            result_xdr: result.get("resultXdr").and_then(|s| s.as_str()).map(|s| s.to_string()),
        })
    }

    /// Submit then wait until the tx is included (or timeout).
    pub async fn send_transaction_and_wait(
        &self,
        signed_tx_xdr: &str,
        max_wait_secs: u64,
    ) -> Result<SendTransactionResult> {
        let mut attempts = 0u32;
        let send = loop {
            let result = self.send_transaction(signed_tx_xdr).await?;
            if result.status == "TRY_AGAIN_LATER" && attempts < 30 {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            break result;
        };

        if send.status == "ERROR" {
            return Ok(send);
        }
        if send.hash.is_empty() {
            return Err(anyhow!("sendTransaction returned empty hash"));
        }
        if send.status != "PENDING" && send.status != "DUPLICATE" {
            return Ok(send);
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(max_wait_secs);
        loop {
            let got = self.get_transaction(&send.hash).await?;
            match got.status.as_str() {
                "SUCCESS" => {
                    return Ok(SendTransactionResult {
                        status: "SUCCESS".to_string(),
                        hash: send.hash,
                        error_result_xdr: None,
                    });
                }
                "FAILED" => {
                    return Ok(SendTransactionResult {
                        status: "FAILED".to_string(),
                        hash: send.hash,
                        error_result_xdr: send.error_result_xdr,
                    });
                }
                // NOT_FOUND / PENDING — keep polling
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                return Ok(SendTransactionResult {
                    status: "TIMEOUT".to_string(),
                    hash: send.hash,
                    error_result_xdr: None,
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }
}

#[derive(Debug)]
pub struct LedgerEntryResult {
    pub entry: xdr::LedgerEntry,
}

#[derive(Debug, Clone)]
pub struct SendTransactionResult {
    pub status: String,
    pub hash: String,
    pub error_result_xdr: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GetTransactionResult {
    pub status: String,
    pub envelope_xdr: Option<String>,
    pub result_xdr: Option<String>,
}

// ===== ScVal extraction helpers =====

/// Extract u32 from ScVal
pub fn scval_to_u32(val: &xdr::ScVal) -> Result<u32> {
    match val {
        xdr::ScVal::U32(v) => Ok(*v),
        _ => Err(anyhow!("Expected U32, got {:?}", std::mem::discriminant(val))),
    }
}

/// Parse fee fields that may be U32/I32/U64/I64/I128 depending on contract.
/// Used for Phoenix `total_fee_bps`, Aquarius `get_fee_fraction`, Sushi `fee`,
/// etc.
pub fn parse_fee_bps_u32(val: &xdr::ScVal) -> Option<u32> {
    match val {
        xdr::ScVal::U32(v) => Some(*v),
        xdr::ScVal::I32(v) if *v >= 0 => Some(*v as u32),
        xdr::ScVal::U64(v) if *v <= u32::MAX as u64 => Some(*v as u32),
        xdr::ScVal::I64(v) if *v >= 0 && *v <= u32::MAX as i64 => Some(*v as u32),
        _ => scval_to_i128(val).ok().and_then(|v| u32::try_from(v).ok()),
    }
}

/// Extract u128 from ScVal
pub fn scval_to_u128(val: &xdr::ScVal) -> Result<u128> {
    match val {
        xdr::ScVal::U128(parts) => Ok(((parts.hi as u128) << 64) | (parts.lo as u128)),
        _ => Err(anyhow!("Expected U128, got {:?}", std::mem::discriminant(val))),
    }
}

/// Extract i128 from ScVal
pub fn scval_to_i128(val: &xdr::ScVal) -> Result<i128> {
    match val {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | (parts.lo as u64 as i128)),
        _ => Err(anyhow!("Expected I128, got {:?}", std::mem::discriminant(val))),
    }
}

/// Extract Address string from ScVal
pub fn scval_to_address(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash)))) => {
            Ok(format!("{}", stellar_strkey::Contract(*hash)))
        }
        xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(
            xdr::Uint256(key),
        )))) => Ok(format!("{}", stellar_strkey::ed25519::PublicKey(*key))),
        _ => Err(anyhow!("Expected Address, got {:?}", std::mem::discriminant(val))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fee_bps_u32_accepts_common_shapes() {
        assert_eq!(parse_fee_bps_u32(&xdr::ScVal::U32(100)), Some(100));
        assert_eq!(parse_fee_bps_u32(&xdr::ScVal::I64(50)), Some(50));
        assert_eq!(
            parse_fee_bps_u32(&xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 3000 })),
            Some(3000)
        );
        assert!(parse_fee_bps_u32(&xdr::ScVal::Symbol("nope".try_into().unwrap())).is_none());
    }
}

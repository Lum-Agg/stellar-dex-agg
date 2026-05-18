//! Comet DEX adapter: Balancer-style weighted pool AMM on Soroban.
//!
//! Comet pools are individually deployed (no factory). Each pool has:
//! - Multiple tokens with different weights
//! - Balancer V1 weighted math for swaps
//!
//! Known pools:
//! - BLND/USDC: CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM (~$4M TVL)
//!
//! Quote approach: read pool state (balances, weights, fee) from chain via getLedgerEntries,
//! then compute output locally using Balancer math (no simulate needed).

use crate::comet_math::{self, CometRecord, STROOP_SCALAR};
use crate::rpc::SorobanRpc;
use crate::traits::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use stellar_xdr::curr::{self as xdr, Limits, WriteXdr, ReadXdr};
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// Known Comet pools on mainnet
const COMET_POOLS: &[(&str, &str, &str)] = &[
    // (pool_address, token_a_contract, token_b_contract)
    (
        "CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM",
        "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV", // BLND
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC
    ),
];

/// Cached pool state for local computation
#[derive(Debug, Clone)]
struct CometPoolState {
    /// Token address -> (balance, weight, scalar)
    records: HashMap<String, CometRecord>,
    swap_fee: i128,
}

pub struct CometAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
    /// Pool address -> pool state
    pool_states: RwLock<HashMap<String, CometPoolState>>,
}

impl CometAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
            pool_states: RwLock::new(HashMap::new()),
        }
    }

    /// Read pool instance data from chain and parse records + fee.
    async fn fetch_pool_state(&self, pool_address: &str) -> Result<CometPoolState> {
        // Read the pool's instance storage via getLedgerEntries
        let contract_hash = stellar_strkey::Contract::from_string(pool_address)
            .map_err(|e| anyhow!("Invalid pool address: {:?}", e))?.0;

        let ledger_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
            contract: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
            key: xdr::ScVal::LedgerKeyContractInstance,
            durability: xdr::ContractDataDurability::Persistent,
        });

        let key_b64 = ledger_key.to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR encode: {:?}", e))?;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": [key_b64] }
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        let resp = client.post(self.rpc.url()).json(&body).send().await?;
        let resp_json: serde_json::Value = resp.json().await?;

        let entries = resp_json
            .get("result")
            .and_then(|r| r.get("entries"))
            .and_then(|e| e.as_array())
            .ok_or_else(|| anyhow!("No entries in response"))?;

        if entries.is_empty() {
            return Err(anyhow!("Pool not found"));
        }

        let xdr_b64 = entries[0].get("xdr").and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("No xdr"))?;

        self.parse_pool_instance(xdr_b64)
    }

    /// Parse the instance storage to extract AllRecordData and SwapFee.
    fn parse_pool_instance(&self, xdr_b64: &str) -> Result<CometPoolState> {
        // Try decoding as different types (RPC returns LedgerEntryData or ContractDataEntry)
        let data = if let Ok(entry) = xdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none()) {
            entry.data
        } else if let Ok(data) = xdr::LedgerEntryData::from_xdr_base64(xdr_b64, Limits::none()) {
            data
        } else if let Ok(cd) = xdr::ContractDataEntry::from_xdr_base64(xdr_b64, Limits::none()) {
            xdr::LedgerEntryData::ContractData(cd)
        } else {
            return Err(anyhow!("Cannot decode XDR"));
        };

        let contract_data = match &data {
            xdr::LedgerEntryData::ContractData(cd) => cd,
            _ => return Err(anyhow!("Not ContractData")),
        };

        let instance = match &contract_data.val {
            xdr::ScVal::ContractInstance(inst) => inst,
            _ => return Err(anyhow!("Not ContractInstance")),
        };

        let storage = match &instance.storage {
            Some(map) => map,
            None => return Err(anyhow!("No storage")),
        };

        let mut swap_fee: i128 = 30_000; // default 0.3%
        let mut records: HashMap<String, CometRecord> = HashMap::new();

        for entry in storage.0.iter() {
            // DataKey::SwapFee is encoded as Vec([Symbol("SwapFee")])
            if is_data_key(&entry.key, "SwapFee") {
                if let Ok(fee) = extract_i128(&entry.val) {
                    swap_fee = fee;
                }
            }

            // DataKey::AllRecordData is encoded as Vec([Symbol("AllRecordData")])
            if is_data_key(&entry.key, "AllRecordData") {
                if let xdr::ScVal::Map(Some(map)) = &entry.val {
                    for record_entry in map.0.iter() {
                        if let Ok(addr) = crate::rpc::scval_to_address(&record_entry.key) {
                            if let Some(record) = parse_record(&record_entry.val) {
                                records.insert(addr, record);
                            }
                        }
                    }
                }
            }
        }

        if records.is_empty() {
            return Err(anyhow!("No records found in pool instance"));
        }

        debug!("Comet pool: {} tokens, fee={}", records.len(), swap_fee);
        Ok(CometPoolState { records, swap_fee })
    }
}

/// Check if a ScVal key matches a DataKey enum variant name.
/// Comet DataKey is encoded as Vec([Symbol("VariantName")]) in soroban-sdk v22+
fn is_data_key(key: &xdr::ScVal, name: &str) -> bool {
    match key {
        xdr::ScVal::Vec(Some(vec)) if !vec.0.is_empty() => {
            match &vec.0[0] {
                xdr::ScVal::Symbol(s) => s.to_string() == name,
                _ => false,
            }
        }
        // Also try direct Symbol (older encoding)
        xdr::ScVal::Symbol(s) => s.to_string() == name,
        _ => false,
    }
}

/// Parse a Record from ScVal (Map with balance, weight, scalar, index fields)
fn parse_record(val: &xdr::ScVal) -> Option<CometRecord> {
    let map = match val {
        xdr::ScVal::Map(Some(m)) => m,
        _ => return None,
    };

    let mut balance: Option<i128> = None;
    let mut weight: Option<i128> = None;
    let mut scalar: Option<i128> = None;

    for entry in map.0.iter() {
        let key_name = match &entry.key {
            xdr::ScVal::Symbol(s) => s.to_string(),
            _ => continue,
        };

        match key_name.as_str() {
            "balance" => balance = extract_i128(&entry.val).ok(),
            "weight" => weight = extract_i128(&entry.val).ok(),
            "scalar" => scalar = extract_i128(&entry.val).ok(),
            _ => {}
        }
    }

    Some(CometRecord {
        balance: balance?,
        weight: weight?,
        scalar: scalar.unwrap_or(STROOP_SCALAR),
    })
}

fn extract_i128(val: &xdr::ScVal) -> Result<i128> {
    match val {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | (parts.lo as u64 as i128)),
        xdr::ScVal::U128(parts) => Ok(((parts.hi as u128) << 64 | parts.lo as u128) as i128),
        xdr::ScVal::I64(v) => Ok(*v as i128),
        xdr::ScVal::U64(v) => Ok(*v as i128),
        xdr::ScVal::U32(v) => Ok(*v as i128),
        xdr::ScVal::I32(v) => Ok(*v as i128),
        _ => Err(anyhow!("Not a number")),
    }
}

#[async_trait]
impl DexAdapter for CometAdapter {
    fn id(&self) -> &str {
        "comet"
    }

    fn name(&self) -> &str {
        "Comet"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanWeightedPool
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let mut pairs = Vec::new();
        let mut states = HashMap::new();

        for (pool_addr, token_a, token_b) in COMET_POOLS {
            match self.fetch_pool_state(pool_addr).await {
                Ok(state) => {
                    // Get balances from state for the pair
                    let reserve_a = state.records.get(*token_a).map(|r| r.balance as u128);
                    let reserve_b = state.records.get(*token_b).map(|r| r.balance as u128);

                    pairs.push(AdapterTradingPair {
                        token_a: TokenId::Contract { address: token_a.to_string() },
                        token_b: TokenId::Contract { address: token_b.to_string() },
                        pool_address: pool_addr.to_string(),
                        fee_bps: (state.swap_fee / 1000) as u32, // convert from stroops to bps
                        reserve_a,
                        reserve_b,
                    });

                    states.insert(pool_addr.to_string(), state);
                }
                Err(e) => {
                    warn!("Comet pool {} fetch failed: {}", pool_addr, e);
                }
            }
        }

        info!("Comet: {} pools loaded with state", pairs.len());
        *self.pairs.write().await = pairs.clone();
        *self.pool_states.write().await = states;
        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let states = self.pool_states.read().await;
        let state = match states.get(pool_address) {
            Some(s) => s,
            None => return Ok(None),
        };

        let token_in_addr = token_in.canonical();
        let token_out_addr = token_out.canonical();

        let in_record = match state.records.get(&token_in_addr) {
            Some(r) => r,
            None => return Ok(None),
        };
        let out_record = match state.records.get(&token_out_addr) {
            Some(r) => r,
            None => return Ok(None),
        };

        let amount_out = comet_math::calc_out_given_in(
            in_record,
            out_record,
            amount_in as i128,
            state.swap_fee,
        );

        if amount_out <= 0 {
            return Ok(None);
        }

        let price_impact_bps = (amount_in as i128 * 10_000 / (2 * in_record.balance)).min(10_000) as u32;

        Ok(Some(AdapterQuote {
            amount_out: amount_out as u128,
            fee_bps: (state.swap_fee / 1000) as u32,
            price_impact_bps,
        }))
    }

    async fn build_swap_op(
        &self,
        _token_in: &TokenId,
        _token_out: &TokenId,
        _amount_in: u128,
        _min_amount_out: u128,
        pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::SorobanInvoke {
            contract_id: pool_address.to_string(),
            function_name: "swap_exact_amount_in".to_string(),
            args_xdr: vec![],
        })
    }

    async fn health_check(&self) -> bool {
        if let Some((pool, _, _)) = COMET_POOLS.first() {
            self.fetch_pool_state(pool).await.is_ok()
        } else {
            false
        }
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        let mut updated = 0;
        let mut new_states = HashMap::new();

        for (pool_addr, _, _) in COMET_POOLS {
            if let Ok(state) = self.fetch_pool_state(pool_addr).await {
                new_states.insert(pool_addr.to_string(), state);
                updated += 1;
            }
        }

        if updated > 0 {
            *self.pool_states.write().await = new_states;
        }

        Ok(updated)
    }
}

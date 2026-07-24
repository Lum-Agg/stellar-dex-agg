//! Soroban simulate + assemble via RPC (`simulateTransaction` + assemble).

use {
    anyhow::{anyhow, Result},
    soroban_client::{
        network::{NetworkPassphrase, Networks},
        transaction::{assemble_transaction, AccountBehavior, TransactionBehavior},
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior, TIMEOUT_INFINITE},
        xdr::{self, Limits, ReadXdr, WriteXdr},
        Options, Server,
    },
    std::{
        sync::Mutex,
        time::{Duration, Instant},
    },
    stellar_xdr::{
        curr as sxdr,
        curr::{Limits as StellarLimits, WriteXdr as StellarWriteXdr},
    },
};

/// Successful simulate + assemble: unsigned XDR and on-chain return value
/// (`base_total`).
#[derive(Debug, Clone)]
pub struct PreparedSimulation {
    pub unsigned_tx_xdr: String,
    pub amount_out: u128,
    /// Declared inclusion + `min_resource_fee` (stroops).
    pub estimated_fee_stroops: u128,
    /// Simulated Soroban `min_resource_fee` only (stroops).
    pub resource_fee_stroops: u128,
}

fn scval_i128_to_u128(val: &xdr::ScVal) -> Option<u128> {
    match val {
        xdr::ScVal::I128(parts) => {
            let v = ((parts.hi as i128) << 64) | (parts.lo as u64 as i128);
            u128::try_from(v).ok()
        }
        _ => None,
    }
}

pub fn default_rpc_url() -> String {
    std::env::var("RPC_URL").unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string())
}

pub fn rpc_server(rpc_url: &str) -> Result<Server> {
    Server::new(
        rpc_url,
        Options {
            allow_http: true,
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("create Soroban RPC client: {}", e))
}

/// Read account ledger entry XDR via Soroban RPC `getLedgerEntries`.
async fn fetch_account_entry_xdr(rpc_url: &str, public_key: &str) -> Result<String> {
    use {
        serde_json::json,
        stellar_strkey::ed25519::PublicKey,
        stellar_xdr::curr::{Limits, WriteXdr},
    };

    let pk = PublicKey::from_string(public_key).map_err(|e| anyhow!("invalid public key: {:?}", e))?;
    let account_id = sxdr::AccountId(sxdr::PublicKey::PublicKeyTypeEd25519(sxdr::Uint256(pk.0)));
    let key = sxdr::LedgerKey::Account(sxdr::LedgerKeyAccount { account_id });
    let key_b64 = key
        .to_xdr_base64(Limits::none())
        .map_err(|e| anyhow!("encode account ledger key: {:?}", e))?;

    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLedgerEntries",
        "params": { "keys": [key_b64] }
    });

    let resp: serde_json::Value = reqwest::Client::new()
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("RPC getLedgerEntries request: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow!("RPC getLedgerEntries JSON: {}", e))?;

    if let Some(error) = resp.get("error") {
        return Err(anyhow!("RPC getLedgerEntries error: {}", error));
    }

    resp.pointer("/result/entries/0/xdr")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("account not found on ledger: {public_key}"))
}

fn decode_account_entry_xdr(xdr_b64: &str) -> Result<sxdr::AccountEntry> {
    use stellar_xdr::curr::{Limits, ReadXdr};

    if let Ok(entry) = sxdr::LedgerEntry::from_xdr_base64(xdr_b64, Limits::none()) {
        if let sxdr::LedgerEntryData::Account(data) = entry.data {
            return Ok(data);
        }
    }
    if let Ok(data) = sxdr::LedgerEntryData::from_xdr_base64(xdr_b64, Limits::none()) {
        if let sxdr::LedgerEntryData::Account(data) = data {
            return Ok(data);
        }
    }
    if let Ok(data) = sxdr::AccountEntry::from_xdr_base64(xdr_b64, Limits::none()) {
        return Ok(data);
    }
    Err(anyhow!("cannot decode account entry from ledger XDR"))
}

fn decode_account_sequence_xdr(xdr_b64: &str) -> Result<i64> {
    Ok(decode_account_entry_xdr(xdr_b64)?.seq_num.0)
}

/// Read the account sequence from Soroban RPC (`getLedgerEntries`). Arb is
/// Soroban-only — no Horizon / SDEX.
pub async fn fetch_account_sequence(rpc_url: &str, public_key: &str) -> Result<i64> {
    let xdr_b64 = fetch_account_entry_xdr(rpc_url, public_key).await?;
    decode_account_sequence_xdr(&xdr_b64)
}

/// Latest closed ledger sequence from Soroban RPC (`getLatestLedger`).
pub async fn fetch_latest_ledger(rpc_url: &str) -> Result<u32> {
    use serde_json::json;

    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestLedger"
    });

    let resp: serde_json::Value = reqwest::Client::new()
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("RPC getLatestLedger request: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow!("RPC getLatestLedger JSON: {}", e))?;

    if let Some(error) = resp.get("error") {
        return Err(anyhow!("RPC getLatestLedger error: {}", error));
    }

    resp.pointer("/result/sequence")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .ok_or_else(|| anyhow!("getLatestLedger missing result.sequence"))
}

/// Ledger cushion for vault reclaim `approve` expiry.
///
/// Passed into `vault.execute_round_trip` as `allowance_expiration_ledger` so
/// the value is fixed in the op/auth tree (same at simulate and inclusion).
///
/// **Do not** move this into the vault as `env.ledger().sequence() + N`:
/// that drifted by 1–2 ledgers on mainnet (2026-07-16) and surfaced as
/// `Unauthorized function call for address` on the nested SAC `approve`
/// while simulate still succeeded. Also avoid `u32::MAX` (SAC max TTL).
///
/// ~100k ledgers is well under typical max entry lifetime (~1M).
pub const VAULT_ALLOWANCE_LEDGER_CUSHION: u32 = 100_000;

/// How long a cached `getLatestLedger` stays valid for allowance expiry.
/// Cushion is ~100k ledgers; being minutes stale is fine and avoids an RPC
/// per prepare under arb load.
pub const LATEST_LEDGER_CACHE_TTL: Duration = Duration::from_secs(60);

pub fn vault_allowance_expiration(latest_ledger: u32) -> u32 {
    latest_ledger.saturating_add(VAULT_ALLOWANCE_LEDGER_CUSHION)
}

/// Shared `getLatestLedger` cache (process / runtime scoped).
#[derive(Debug, Default)]
pub struct LatestLedgerCache {
    inner: Mutex<Option<(u32, Instant)>>,
}

impl LatestLedgerCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a recent ledger sequence, refreshing via RPC when the cache is
    /// empty or older than [`LATEST_LEDGER_CACHE_TTL`].
    pub async fn get(&self, rpc_url: &str) -> Result<u32> {
        if let Some(seq) = self.cached() {
            return Ok(seq);
        }
        let seq = fetch_latest_ledger(rpc_url).await?;
        if let Ok(mut g) = self.inner.lock() {
            *g = Some((seq, Instant::now()));
        }
        Ok(seq)
    }

    fn cached(&self) -> Option<u32> {
        let g = self.inner.lock().ok()?;
        let (seq, at) = (*g)?;
        (at.elapsed() < LATEST_LEDGER_CACHE_TTL).then_some(seq)
    }
}

/// `latest + cushion` using a shared ledger cache (no RPC on cache hit).
pub async fn vault_allowance_expiration_cached(cache: &LatestLedgerCache, rpc_url: &str) -> Result<u32> {
    Ok(vault_allowance_expiration(cache.get(rpc_url).await?))
}

/// Native XLM balance (stroops) for a G... account via Soroban RPC.
pub async fn fetch_account_native_balance(rpc_url: &str, public_key: &str) -> Result<u128> {
    let xdr_b64 = fetch_account_entry_xdr(rpc_url, public_key).await?;
    let entry = decode_account_entry_xdr(&xdr_b64)?;
    u128::try_from(entry.balance).map_err(|_| anyhow!("negative account balance"))
}

/// Simulate + assemble footprint/auth; return unsigned envelope XDR and
/// contract return value (`base_total` for round-trip ops).
pub async fn prepare_transaction_xdr(
    rpc_url: &str,
    public_key: &str,
    sequence: u64,
    operations: &[sxdr::Operation],
    fee: u32,
) -> Result<PreparedSimulation> {
    let mut account = soroban_client::account::Account::new(public_key, &sequence.to_string())
        .map_err(|e| anyhow!("invalid account/sequence: {}", e))?;

    let mut builder = TransactionBuilder::new(&mut account, Networks::public(), None);
    builder.fee(fee);

    for op in operations {
        let op_bytes = op
            .to_xdr(StellarLimits::none())
            .map_err(|e| anyhow!("encode operation: {:?}", e))?;
        let client_op = xdr::Operation::from_xdr(op_bytes, Limits::none())
            .map_err(|e| anyhow!("decode operation for soroban_client: {:?}", e))?;
        builder.add_operation(client_op);
    }

    let tx = builder
        .set_timeout(TIMEOUT_INFINITE)
        .map_err(|e| anyhow!("timeout: {}", e))?
        .build();

    let server = rpc_server(rpc_url)?;
    let sim_response = server
        .simulate_transaction(&tx, None)
        .await
        .map_err(|e| anyhow!("simulate_transaction: {:?}", e))?;

    let resource_fee: u128 = sim_response
        .min_resource_fee
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let amount_out = match sim_response.to_result().and_then(|(ret, _)| scval_i128_to_u128(&ret)) {
        Some(v) => v,
        None => {
            let detail = sim_response
                .error
                .clone()
                .unwrap_or_else(|| "no simulation error detail".to_string());
            return Err(anyhow!(
                "simulation missing i128 return value (resource_fee_stroops={resource_fee}): {detail}"
            ));
        }
    };
    let estimated_fee_stroops = u128::from(fee).saturating_add(resource_fee);

    let prepared = assemble_transaction(&tx, sim_response).map_err(|e| anyhow!("assemble_transaction: {:?}", e))?;

    let envelope = prepared.to_envelope().map_err(|e| anyhow!("to_envelope: {}", e))?;

    let unsigned_tx_xdr = envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| anyhow!("XDR encode: {:?}", e))?;

    Ok(PreparedSimulation {
        unsigned_tx_xdr,
        amount_out,
        estimated_fee_stroops,
        resource_fee_stroops: resource_fee,
    })
}

/// Sum unique token transfers to `aggregator` for `token` (dedup identical
/// lines).
fn parse_token_received_by_aggregator(error: &str, token: &str, aggregator: &str) -> Option<u128> {
    let contract_tag = format!("contract:{token}");
    let mut total = 0u128;
    let mut saw = false;
    let mut seen_lines = std::collections::HashSet::new();
    for line in error.lines() {
        if !seen_lines.insert(line.to_string()) {
            continue;
        }
        if !line.contains(&contract_tag) || !line.contains("topics:[transfer,") {
            continue;
        }
        // topics:[transfer, FROM, TO, ...] — require TO == aggregator.
        let Some(topics) = line.split("topics:[").nth(1).and_then(|s| s.split(']').next()) else {
            continue;
        };
        let parts: Vec<&str> = topics.split(',').map(str::trim).collect();
        if parts.len() < 3 || parts[0] != "transfer" {
            continue;
        }
        if parts[2] != aggregator {
            continue;
        }
        let Some(rest) = line.split("data:").nth(1) else {
            continue;
        };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(amt) = digits.parse::<u128>() {
            total = total.saturating_add(amt);
            saw = true;
        }
    }
    saw.then_some(total)
}

/// Bridge output from leg_out: sum deduped bridge-token transfers to the
/// aggregator, or fall back to the last `fn_return, swap` before leg_back.
pub fn parse_bridge_received_from_sim_error(error: &str, bridge_token: &str, aggregator: &str) -> Option<u128> {
    if let Some(total) = parse_token_received_by_aggregator(error, bridge_token, aggregator) {
        return Some(total);
    }

    // Single-hop leg_out: pool swap return equals bridge received.
    let mut last_swap_out = None;
    for line in error.lines() {
        if line.contains("topics:[fn_return, swap]") && line.contains("data:") {
            let Some(rest) = line.split("data:").nth(1) else {
                continue;
            };
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(amt) = digits.parse::<u128>() {
                last_swap_out = Some(amt);
            }
        }
    }
    last_swap_out
}

/// Base token returned by leg_back pool swaps (transfers into aggregator),
/// excluding the initial caller→aggregator principal pull.
pub fn parse_base_received_from_sim_error(
    error: &str,
    base_token: &str,
    aggregator: &str,
    caller: &str,
) -> Option<u128> {
    let contract_tag = format!("contract:{base_token}");
    let mut total = 0u128;
    let mut saw = false;
    let mut seen_lines = std::collections::HashSet::new();
    for line in error.lines() {
        if !seen_lines.insert(line.to_string()) {
            continue;
        }
        if !line.contains(&contract_tag) || !line.contains("topics:[transfer,") {
            continue;
        }
        let Some(topics) = line.split("topics:[").nth(1).and_then(|s| s.split(']').next()) else {
            continue;
        };
        let parts: Vec<&str> = topics.split(',').map(str::trim).collect();
        if parts.len() < 3 || parts[0] != "transfer" {
            continue;
        }
        // Only pool → aggregator returns (skip caller → aggregator pull).
        if parts[2] != aggregator || parts[1] == caller {
            continue;
        }
        let Some(rest) = line.split("data:").nth(1) else {
            continue;
        };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(amt) = digits.parse::<u128>() {
            total = total.saturating_add(amt);
            saw = true;
        }
    }
    saw.then_some(total)
}

#[cfg(test)]
mod bridge_parse_tests {
    use super::parse_bridge_received_from_sim_error;

    #[test]
    fn sums_bridge_transfers_to_aggregator() {
        let log = r#"   13: [Failed Contract Event] contract:CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75, topics:[transfer, POOL, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "USDC"], data:18252396"#;
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let bridge = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        assert_eq!(parse_bridge_received_from_sim_error(log, bridge, agg), Some(18_252_396));
    }

    #[test]
    fn parses_production_usdc_leg_out_mismatch() {
        let log = r#"   9: [Failed Contract Event (not emitted)] contract:CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75, topics:[transfer, CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"], data:1014904660"#;
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let bridge = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        assert_eq!(
            parse_bridge_received_from_sim_error(log, bridge, agg),
            Some(1_014_904_660)
        );
    }

    #[test]
    fn dedupes_duplicate_transfer_lines_in_event_log() {
        let line = r#"   9: [Failed Contract Event (not emitted)] contract:CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75, topics:[transfer, POOL, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "USDC"], data:18372538"#;
        let log = format!("{line}\n{line}");
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let bridge = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        assert_eq!(
            parse_bridge_received_from_sim_error(&log, bridge, agg),
            Some(18_372_538)
        );
    }

    #[test]
    fn falls_back_to_swap_return_when_no_transfer_line() {
        let log = r#"   4: [Failed Diagnostic Event (not emitted)] contract:CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV, topics:[fn_return, swap], data:18372538"#;
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let bridge = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        assert_eq!(parse_bridge_received_from_sim_error(log, bridge, agg), Some(18_372_538));
    }

    #[test]
    fn parses_base_out_excluding_caller_pull() {
        use super::parse_base_received_from_sim_error;
        let log = r#"
   10: [Failed Contract Event] contract:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA, topics:[transfer, CCY2PXGMKNQHO7WNYXEWX76L2C5BH3JUW3RCATGUYKY7QQTRILBZIFWV, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "native"], data:841208923
   16: [Failed Contract Event] contract:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA, topics:[transfer, GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "native"], data:1832045401
   32: [Failed Contract Event] contract:CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA, topics:[transfer, CA4HTZNY2RBZWEQE5GBMNREZMFRPAZSVJ6OGPC7T3VM7NHRJYFAVID2S, CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K, "native"], data:984439379
"#;
        let agg = "CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K";
        let base = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
        let caller = "GCMDWFAHD6PYI5SI2N2M6XINZDITECUV4XN7LYQGOWKQSIMQPRNK2DLN";
        assert_eq!(
            parse_base_received_from_sim_error(log, base, agg, caller),
            Some(841_208_923 + 984_439_379)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_i128_return_as_u128() {
        let val = xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 100_144_152 });
        assert_eq!(scval_i128_to_u128(&val), Some(100_144_152));
    }

    #[test]
    fn decodes_account_sequence_from_ledger_entry_data_xdr() {
        use stellar_xdr::curr::{Limits, WriteXdr};

        let entry = sxdr::AccountEntry {
            account_id: sxdr::AccountId(sxdr::PublicKey::PublicKeyTypeEd25519(sxdr::Uint256([1u8; 32]))),
            balance: 1,
            seq_num: sxdr::SequenceNumber(42),
            num_sub_entries: 0,
            inflation_dest: None,
            flags: 0,
            home_domain: sxdr::String32::default(),
            thresholds: sxdr::Thresholds([0; 4]),
            signers: sxdr::VecM::default(),
            ext: sxdr::AccountEntryExt::V0,
        };
        let data = sxdr::LedgerEntryData::Account(entry);
        let b64 = data.to_xdr_base64(Limits::none()).unwrap();
        assert_eq!(super::decode_account_sequence_xdr(&b64).unwrap(), 42);
    }

    #[test]
    fn latest_ledger_cache_hits_within_ttl() {
        let cache = LatestLedgerCache::new();
        assert!(cache.cached().is_none());
        *cache.inner.lock().unwrap() = Some((63_500_000, Instant::now()));
        assert_eq!(cache.cached(), Some(63_500_000));
        assert_eq!(
            vault_allowance_expiration(63_500_000),
            63_500_000 + VAULT_ALLOWANCE_LEDGER_CUSHION
        );
    }
}

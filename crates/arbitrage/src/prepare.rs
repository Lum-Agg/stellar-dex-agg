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

pub async fn fetch_account_sequence(horizon_url: &str, public_key: &str) -> Result<i64> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), public_key);
    let data: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Horizon account request: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow!("Horizon account JSON: {}", e))?;
    let seq_str = data
        .get("sequence")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow!("missing sequence in Horizon account"))?;
    seq_str.parse().map_err(|e| anyhow!("parse sequence: {}", e))
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

    let amount_out = match sim_response.to_result().and_then(|(ret, _)| scval_i128_to_u128(&ret)) {
        Some(v) => v,
        None => {
            let detail = sim_response
                .error
                .clone()
                .unwrap_or_else(|| "no simulation error detail".to_string());
            return Err(anyhow!("simulation missing i128 return value: {detail}"));
        }
    };

    let prepared = assemble_transaction(&tx, sim_response).map_err(|e| anyhow!("assemble_transaction: {:?}", e))?;

    let envelope = prepared.to_envelope().map_err(|e| anyhow!("to_envelope: {}", e))?;

    let unsigned_tx_xdr = envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| anyhow!("XDR encode: {:?}", e))?;

    Ok(PreparedSimulation {
        unsigned_tx_xdr,
        amount_out,
    })
}

/// Sum bridge-token transfers to the aggregator seen in a failed simulation
/// log.
pub fn parse_bridge_received_from_sim_error(error: &str, bridge_token: &str, aggregator: &str) -> Option<u128> {
    let contract_tag = format!("contract:{bridge_token}");
    let mut total = 0u128;
    let mut saw = false;
    for line in error.lines() {
        if !line.contains(&contract_tag) || !line.contains("topics:[transfer,") {
            continue;
        }
        if !line.contains(aggregator) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_i128_return_as_u128() {
        let val = xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 100_144_152 });
        assert_eq!(scval_i128_to_u128(&val), Some(100_144_152));
    }
}

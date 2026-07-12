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

    let amount_out = sim_response
        .to_result()
        .and_then(|(ret, _)| scval_i128_to_u128(&ret))
        .ok_or_else(|| anyhow!("simulation missing i128 return value"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_i128_return_as_u128() {
        let val = xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 100_144_152 });
        assert_eq!(scval_i128_to_u128(&val), Some(100_144_152));
    }
}

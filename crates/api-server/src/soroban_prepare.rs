//! Simulate + assemble Soroban transactions via
//! `soroban_client::Server::prepare_transaction`. Same approach as stellar-arb
//! (`STELLAR_CLIENT.prepare_transaction`).

use {
    soroban_client::{
        network::{NetworkPassphrase, Networks},
        transaction::{AccountBehavior, TransactionBehavior},
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior, TIMEOUT_INFINITE},
        xdr::{self, Limits, ReadXdr, WriteXdr},
        Options, Server,
    },
    stellar_xdr::{
        curr as sxdr,
        curr::{Limits as StellarLimits, WriteXdr as StellarWriteXdr},
    },
};

pub fn rpc_server(rpc_url: &str) -> Result<Server, String> {
    Server::new(
        rpc_url,
        Options {
            allow_http: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Soroban RPC client: {}", e))
}

/// Resolve passphrase for Soroban tx build. Defaults to public (mainnet
/// Instant).
pub fn network_passphrase_from_env() -> &'static str {
    match std::env::var("NETWORK_PASSPHRASE").ok().as_deref() {
        Some(p) if p == Networks::testnet() || p.contains("Test SDF") => Networks::testnet(),
        Some(p) if p == Networks::public() || p.contains("Public Global") => Networks::public(),
        // KEEPER-style short names
        Some("testnet") => Networks::testnet(),
        Some("public") | None => Networks::public(),
        Some(_) => Networks::public(),
    }
}

/// Run simulate + assemble (footprint, auth, resource fee) and return unsigned
/// envelope XDR. `sequence` must be the account's current on-chain sequence;
/// the builder increments it.
pub async fn prepare_transaction_xdr(
    rpc_url: &str,
    user_public_key: &str,
    sequence: u64,
    operations: &[sxdr::Operation],
    fee: u32,
) -> Result<String, String> {
    prepare_transaction_xdr_on_network(rpc_url, user_public_key, sequence, operations, fee, Networks::public()).await
}

/// Same as [`prepare_transaction_xdr`] but with an explicit network passphrase
/// (use [`Networks::testnet`] for Phase 3d Limit on testnet).
pub async fn prepare_transaction_xdr_on_network(
    rpc_url: &str,
    user_public_key: &str,
    sequence: u64,
    operations: &[sxdr::Operation],
    fee: u32,
    network_passphrase: &str,
) -> Result<String, String> {
    let mut account = soroban_client::account::Account::new(user_public_key, &sequence.to_string())
        .map_err(|e| format!("Invalid account/sequence: {}", e))?;

    let mut builder = TransactionBuilder::new(&mut account, network_passphrase, None);
    builder.fee(fee);

    for op in operations {
        let op_bytes = op
            .to_xdr(StellarLimits::none())
            .map_err(|e| format!("encode operation: {:?}", e))?;
        let client_op = xdr::Operation::from_xdr(op_bytes, Limits::none())
            .map_err(|e| format!("decode operation for soroban_client: {:?}", e))?;
        builder.add_operation(client_op);
    }

    let tx = builder
        .set_timeout(TIMEOUT_INFINITE)
        .map_err(|e| format!("timeout: {}", e))?
        .build();

    let server = rpc_server(rpc_url)?;
    let prepared = server
        .prepare_transaction(&tx)
        .await
        .map_err(|e| format!("prepare_transaction: {:?}", e))?;

    let envelope = prepared.to_envelope().map_err(|e| format!("to_envelope: {}", e))?;

    envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| format!("XDR encode: {:?}", e))
}

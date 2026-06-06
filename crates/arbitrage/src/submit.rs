//! Sign prepared XDR and submit to Soroban RPC (stellar-arb tx_executor
//! pattern).

use {
    crate::{execute::PreparedArbTx, keypair::ExecutorKeypair, prepare::rpc_server, stats::ArbStats},
    anyhow::{anyhow, Result},
    soroban_client::{
        network::{NetworkPassphrase, Networks},
        soroban_rpc::{SendTransactionStatus, TransactionStatus},
        transaction::TransactionBehavior,
    },
    std::{sync::atomic::Ordering, time::Duration},
    tracing::{error, info, warn},
};

const POLL_ATTEMPTS: usize = 10;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Sign Soroban-prepared envelope XDR and broadcast.
pub async fn sign_and_submit(rpc_url: &str, keypair: &ExecutorKeypair, unsigned_xdr: &str) -> Result<String> {
    let mut tx = stellar_baselib::transaction::Transaction::from_xdr_envelope(unsigned_xdr, Networks::public());
    tx.sign(&[keypair.inner().clone()]);

    let server = rpc_server(rpc_url)?;
    let result = server
        .send_transaction(tx)
        .await
        .map_err(|e| anyhow!("send_transaction: {:?}", e))?;

    if result.status == SendTransactionStatus::Error {
        let detail = result
            .to_error_result()
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|| "unknown".to_string());
        return Err(anyhow!("tx rejected immediately: {}", detail));
    }

    Ok(result.hash)
}

pub async fn poll_transaction(rpc_url: &str, hash: &str) -> Result<()> {
    let server = rpc_server(rpc_url)?;
    for _ in 0..POLL_ATTEMPTS {
        tokio::time::sleep(POLL_INTERVAL).await;
        match server.get_transaction(hash).await {
            Ok(status) => {
                if status.status == TransactionStatus::Success {
                    return Ok(());
                }
                if status.status == TransactionStatus::Failed {
                    let reason = status
                        .to_result()
                        .map(|r| format!("{:?}", r))
                        .unwrap_or_else(|| "unknown".to_string());
                    return Err(anyhow!("tx failed on-chain: {}", reason));
                }
            }
            Err(e) => {
                warn!(hash, error = %e, "get_transaction poll error");
            }
        }
    }
    Err(anyhow!("tx status poll timeout"))
}

/// Full submit + poll; updates stats.
pub async fn submit_prepared(
    rpc_url: &str,
    keypair: &ExecutorKeypair,
    prepared: &PreparedArbTx,
    stats: &ArbStats,
) -> Result<()> {
    if !prepared.simulated {
        return Err(anyhow!(
            "refusing to submit non-simulated tx for route {}",
            prepared.route_label
        ));
    }

    let hash = sign_and_submit(rpc_url, keypair, &prepared.unsigned_tx_xdr).await?;
    stats.txs_submitted.fetch_add(1, Ordering::Relaxed);
    info!(
        hash = %hash,
        route = %prepared.route_label,
        profit_bps = prepared.profit_bps,
        "arb tx submitted"
    );

    match poll_transaction(rpc_url, &hash).await {
        Ok(()) => {
            stats.txs_succeeded.fetch_add(1, Ordering::Relaxed);
            info!(hash = %hash, "arb tx SUCCESS");
            Ok(())
        }
        Err(e) => {
            stats.txs_failed.fetch_add(1, Ordering::Relaxed);
            error!(hash = %hash, error = %e, "arb tx failed");
            Err(e)
        }
    }
}

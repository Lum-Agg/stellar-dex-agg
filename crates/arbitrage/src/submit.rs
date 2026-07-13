//! Sign prepared XDR and submit to Soroban RPC (stellar-arb tx_executor
//! pattern).

use {
    crate::{execute::PreparedArbTx, keypair::ExecutorKeypair, prepare::rpc_server, stats::ArbStats},
    anyhow::{anyhow, Result},
    soroban_client::{
        network::{NetworkPassphrase, Networks},
        soroban_rpc::{SendTransactionStatus, TransactionStatus},
        transaction::{Transaction, TransactionBehavior},
    },
    std::{sync::atomic::Ordering, time::Duration},
    tracing::{error, info, warn},
};

const POLL_ATTEMPTS: usize = 10;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// `stellar_baselib::Transaction::from_xdr_envelope` drops `TransactionExt::V1`
/// (footprint/resource fee). Re-attach so `to_envelope()` stays a valid Soroban
/// tx; otherwise RPC rejects with `TxMalformed`.
fn transaction_from_prepared_xdr(unsigned_xdr: &str) -> Result<Transaction> {
    use stellar_baselib::xdr::{Limits, ReadXdr, TransactionEnvelope, TransactionExt};

    let mut tx = Transaction::from_xdr_envelope(unsigned_xdr, Networks::public());
    let envelope = TransactionEnvelope::from_xdr_base64(unsigned_xdr, Limits::none())
        .map_err(|e| anyhow!("parse prepared envelope: {:?}", e))?;
    if let TransactionEnvelope::Tx(v1) = envelope {
        if let TransactionExt::V1(data) = v1.tx.ext {
            tx.soroban_data = Some(data);
        }
    }
    Ok(tx)
}

/// Sign Soroban-prepared envelope XDR and broadcast.
pub async fn sign_and_submit(rpc_url: &str, keypair: &ExecutorKeypair, unsigned_xdr: &str) -> Result<String> {
    let mut tx = transaction_from_prepared_xdr(unsigned_xdr)?;
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

#[cfg(test)]
mod tests {
    use {
        super::transaction_from_prepared_xdr,
        soroban_client::{
            network::{NetworkPassphrase, Networks},
            transaction::{Transaction, TransactionBehavior},
            xdr::WriteXdr,
        },
        stellar_baselib::xdr::{Limits, ReadXdr, TransactionEnvelope, TransactionExt},
    };

    fn envelope_ext_kind(xdr: &str) -> &'static str {
        let envelope = TransactionEnvelope::from_xdr_base64(xdr, Limits::none()).expect("parse envelope");
        match envelope {
            TransactionEnvelope::Tx(v1) => match v1.tx.ext {
                TransactionExt::V0 => "V0",
                TransactionExt::V1(_) => "V1",
            },
            _ => "other",
        }
    }

    #[test]
    fn restores_soroban_data_from_prepared_envelope() {
        let Ok(sample) = std::env::var("TEST_PREPARED_TX_XDR") else {
            return;
        };
        assert_eq!(envelope_ext_kind(&sample), "V1", "fixture must be assembled Soroban tx");

        let broken = Transaction::from_xdr_envelope(&sample, Networks::public());
        assert!(broken.soroban_data.is_none(), "baselib parser drops soroban_data");
        let broken_xdr = broken
            .to_envelope()
            .expect("broken envelope")
            .to_xdr_base64(Limits::none())
            .expect("encode broken");
        assert_eq!(
            envelope_ext_kind(&broken_xdr),
            "V0",
            "unsigned re-encode without fix loses V1 ext"
        );

        let fixed = transaction_from_prepared_xdr(&sample).expect("parse prepared tx");
        assert!(fixed.soroban_data.is_some(), "fix must restore soroban_data");
        let fixed_xdr = fixed
            .to_envelope()
            .expect("fixed envelope")
            .to_xdr_base64(Limits::none())
            .expect("encode fixed");
        assert_eq!(
            envelope_ext_kind(&fixed_xdr),
            "V1",
            "re-encode with fix keeps Soroban ext"
        );
    }
}

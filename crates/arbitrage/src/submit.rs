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
    std::{
        sync::{atomic::Ordering, Arc},
        time::Duration,
    },
    tracing::{error, info, warn},
};

/// ~one Stellar ledger; poll runs **without** holding caller mutex.
const POLL_ATTEMPTS: usize = 3;
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

    match result.status {
        SendTransactionStatus::Pending | SendTransactionStatus::Duplicate => Ok(result.hash),
        SendTransactionStatus::Error => {
            let detail = result
                .to_error_result()
                .map(|r| format!("{:?}", r))
                .unwrap_or_else(|| "unknown".to_string());
            Err(anyhow!("tx rejected immediately: {}", detail))
        }
        // A real-time arb opportunity is stale when the RPC is overloaded;
        // never retry it and create more submission pressure.
        SendTransactionStatus::TryAgainLater => {
            warn!("RPC returned TRY_AGAIN_LATER; dropping stale arb opportunity");
            Err(anyhow!("RPC returned TRY_AGAIN_LATER; opportunity dropped"))
        }
    }
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

struct PollContext {
    hash: String,
    route_label: String,
    amount_in: u128,
    simulated_amount_out: u128,
    estimated_fee_stroops: u128,
}

/// Poll in background so caller mutex is not held (~6s ledger window).
fn spawn_poll_outcome(rpc_url: String, ctx: PollContext, stats: Arc<ArbStats>, profit: Arc<crate::profit::ProfitBook>) {
    tokio::spawn(async move {
        match poll_transaction(&rpc_url, &ctx.hash).await {
            Ok(()) => {
                stats.txs_succeeded.fetch_add(1, Ordering::Relaxed);
                let gross = ctx.simulated_amount_out.saturating_sub(ctx.amount_in);
                profit.record_success(&ctx.hash, ctx.amount_in, gross, ctx.estimated_fee_stroops);
                info!(
                    hash = %ctx.hash,
                    route = %ctx.route_label,
                    gross_profit = gross,
                    estimated_fee = ctx.estimated_fee_stroops,
                    "arb tx SUCCESS"
                );
            }
            Err(e) => {
                if e.to_string() == "tx status poll timeout" {
                    // A Soroban RPC may not expose the final result within the
                    // short observation window. This is unknown, not failure.
                    warn!(hash = %ctx.hash, "tx status not confirmed within poll window");
                    return;
                }
                stats.txs_failed.fetch_add(1, Ordering::Relaxed);
                profit.record_failed();
                error!(hash = %ctx.hash, route = %ctx.route_label, error = %e, "arb tx failed");
            }
        }
    });
}

/// Sign + broadcast; optionally poll outcome in background (see `ARB_POLL_TX`).
pub async fn submit_prepared(
    rpc_url: &str,
    keypair: &ExecutorKeypair,
    prepared: &PreparedArbTx,
    stats: Arc<ArbStats>,
    profit: Arc<crate::profit::ProfitBook>,
    poll_tx: bool,
) -> Result<()> {
    if !prepared.simulated {
        return Err(anyhow!(
            "refusing to submit non-simulated tx for route {}",
            prepared.route_label
        ));
    }

    let hash = sign_and_submit(rpc_url, keypair, &prepared.unsigned_tx_xdr).await?;
    stats.txs_submitted.fetch_add(1, Ordering::Relaxed);
    profit.record_submitted();
    info!(
        hash = %hash,
        route = %prepared.route_label,
        profit_bps = prepared.profit_bps,
        estimated_fee_stroops = prepared.estimated_fee_stroops,
        "arb tx submitted"
    );

    if poll_tx {
        spawn_poll_outcome(
            rpc_url.to_string(),
            PollContext {
                hash,
                route_label: prepared.route_label.clone(),
                amount_in: prepared.amount_in,
                simulated_amount_out: prepared.simulated_amount_out,
                estimated_fee_stroops: prepared.estimated_fee_stroops,
            },
            stats,
            profit,
        );
    }

    Ok(())
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

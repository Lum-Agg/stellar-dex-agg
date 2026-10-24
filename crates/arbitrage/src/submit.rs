//! Sign prepared XDR and submit to Soroban RPC (stellar-arb tx_executor
//! pattern).

use {
    crate::{
        execute::PreparedArbTx,
        keypair::ExecutorKeypair,
        prepare::rpc_server,
        settlement::{apply_settlement, Settlement},
        stats::ArbStats,
        submission_ledger::{unix_seconds, PendingSubmission, SubmissionLedger},
    },
    anyhow::{anyhow, Result},
    soroban_client::{
        network::{NetworkPassphrase, Networks},
        soroban_rpc::SendTransactionStatus,
        transaction::{Transaction, TransactionBehavior},
    },
    std::{
        sync::{atomic::Ordering, Arc},
        time::Duration,
    },
    tracing::{info, warn},
};

/// Allow several ledgers for RPC indexing before classifying a broadcast as
/// unknown. Poll runs without holding the caller mutex.
const POLL_ATTEMPTS: usize = 15;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(crate) struct BroadcastUncertain(String);

impl std::fmt::Display for BroadcastUncertain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "broadcast outcome unknown: {}", self.0)
    }
}

impl std::error::Error for BroadcastUncertain {}

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
pub async fn sign_and_submit(rpc_url: &str, keypair: &ExecutorKeypair, unsigned_xdr: &str) -> Result<(String, String)> {
    let (tx, signed_xdr) = sign_transaction(keypair, unsigned_xdr)?;
    let hash = broadcast(rpc_url, tx).await?;
    Ok((hash, signed_xdr))
}

fn sign_transaction(keypair: &ExecutorKeypair, unsigned_xdr: &str) -> Result<(Transaction, String)> {
    use soroban_client::xdr::WriteXdr;

    let mut tx = transaction_from_prepared_xdr(unsigned_xdr)?;
    tx.sign(&[keypair.inner().clone()]);
    let signed_xdr = tx
        .to_envelope()
        .map_err(|e| anyhow!("encode signed envelope: {}", e))?
        .to_xdr_base64(stellar_baselib::xdr::Limits::none())
        .map_err(|e| anyhow!("encode signed envelope XDR: {:?}", e))?;

    Ok((tx, signed_xdr))
}

async fn broadcast(rpc_url: &str, tx: Transaction) -> Result<String> {
    let server = rpc_server(rpc_url)?;
    let result = server
        .send_transaction(tx)
        .await
        .map_err(|e| anyhow::Error::new(BroadcastUncertain(format!("send_transaction: {e:?}"))))?;

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

pub async fn poll_transaction(rpc_url: &str, hash: &str) -> Result<Settlement> {
    let server = rpc_server(rpc_url)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    for _ in 0..POLL_ATTEMPTS {
        if tokio::time::timeout_at(deadline, tokio::time::sleep(POLL_INTERVAL))
            .await
            .is_err()
        {
            break;
        }
        let request_deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(5));
        match tokio::time::timeout_at(request_deadline, server.get_transaction(hash)).await {
            Ok(Ok(status)) => {
                if let Some(outcome) = Settlement::from_response(&status) {
                    return Ok(outcome);
                }
            }
            error => {
                warn!(hash, error = ?error, "get_transaction poll error");
            }
        }
    }
    Err(anyhow!("tx status poll timeout"))
}

/// Decode only diagnostic error events from the failed transaction itself.
/// Re-simulating after failure can observe a different pool state and produce
/// a misleading reason, while `getTransaction` preserves the original trap.
pub(crate) fn decode_failure_diagnostics(events: Option<Vec<soroban_client::xdr::DiagnosticEvent>>) -> String {
    let mut messages = Vec::new();
    for event in events.unwrap_or_default() {
        let text = format!("{event:?}");
        let lower = text.to_ascii_lowercase();
        if lower.contains("stringm(error)")
            || lower.contains("error(contract, #")
            || lower.contains("unreachablecodereached")
            || lower.contains("invalidaction")
            || lower.contains("exceededlimit")
        {
            messages.push(text);
        }
    }

    let joined = messages.join(" ");
    // Keep the submission ledger bounded while retaining the trap and route
    // arguments needed to identify the failing venue/pool.
    joined.chars().take(4_000).collect()
}

struct PollContext {
    tx: PendingSubmission,
    submission_ledger: Option<Arc<SubmissionLedger>>,
}

/// Poll in background so caller mutex is not held (~6s ledger window).
fn spawn_poll_outcome(rpc_url: String, ctx: PollContext, stats: Arc<ArbStats>, profit: Arc<crate::profit::ProfitBook>) {
    tokio::spawn(async move {
        match poll_transaction(&rpc_url, &ctx.tx.hash).await {
            Ok(outcome) => {
                if let Err(error) =
                    apply_settlement(&ctx.tx, &outcome, ctx.submission_ledger.as_deref(), &stats, &profit)
                {
                    warn!(hash = %ctx.tx.hash, %error, "settlement failed; recovery worker will retry");
                }
            }
            Err(e) => {
                if e.to_string() == "tx status poll timeout" {
                    // A Soroban RPC may not expose the final result within the
                    // short observation window. This is unknown, not failure.
                    profit.record_unknown(&ctx.tx.base_token);
                    if let Some(ledger) = &ctx.submission_ledger {
                        if let Err(e) = ledger.mark_status(&ctx.tx.hash, "UNKNOWN", Some("poll_timeout")) {
                            warn!(hash = %ctx.tx.hash, error = %e, "failed to update submission ledger");
                        }
                    }
                    warn!(hash = %ctx.tx.hash, "tx status not confirmed within poll window");
                    return;
                }
                warn!(hash = %ctx.tx.hash, error = %e, "status lookup failed; not a chain failure");
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
    submission_ledger: Option<Arc<SubmissionLedger>>,
    poll_tx: bool,
) -> Result<()> {
    if !prepared.simulated {
        return Err(anyhow!(
            "refusing to submit non-simulated tx for route {}",
            prepared.route_label
        ));
    }

    let (signed_tx, _) = sign_transaction(keypair, &prepared.unsigned_tx_xdr)?;
    let hash = hex::encode(signed_tx.hash());
    // Save the hash before sending: a transport error may follow acceptance.
    if let Some(ledger) = &submission_ledger {
        ledger.record_submitted(
            &hash,
            &prepared.caller_public_key,
            &prepared.route_label,
            &prepared.venue_route_label,
            &prepared.pool_route_label,
            prepared.amount_in,
            prepared.quoted_amount_out,
            prepared.simulated_amount_out,
            prepared.estimated_fee_stroops,
            &prepared.base_token,
            prepared.xlm_price_e7,
        )?;
    }
    let uncertain_error = match broadcast(rpc_url, signed_tx).await {
        Ok(_) => None,
        Err(error) if error.is::<BroadcastUncertain>() => Some(error),
        Err(error) => {
            if let Some(ledger) = &submission_ledger {
                ledger.finalize(&hash, "REJECTED", None, Some(0), None, Some(&error.to_string()))?;
            }
            return Err(error);
        }
    };
    if uncertain_error.is_none() {
        stats.txs_submitted.fetch_add(1, Ordering::Relaxed);
        profit.record_submitted(&prepared.base_token);
    }
    info!(
        hash = %hash,
        route = %prepared.route_label,
        profit_bps = prepared.profit_bps,
        estimated_fee_stroops = prepared.estimated_fee_stroops,
        broadcast_uncertain = uncertain_error.is_some(),
        "arb tx broadcast attempt tracked"
    );

    if poll_tx {
        spawn_poll_outcome(
            rpc_url.to_string(),
            PollContext {
                tx: PendingSubmission {
                    base_token: prepared.base_token.clone(),
                    hash,
                    submitted_at: unix_seconds(),
                    route: prepared.route_label.clone(),
                    venue_route: prepared.venue_route_label.clone(),
                    amount_in: prepared.amount_in,
                    simulated_amount_out: prepared.simulated_amount_out,
                    estimated_fee_stroops: prepared.estimated_fee_stroops,
                    xlm_price_e7: prepared.xlm_price_e7,
                },
                submission_ledger,
            },
            stats,
            profit,
        );
    }

    match uncertain_error {
        Some(error) => Err(error),
        None => Ok(()),
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

    #[tokio::test]
    async fn transport_error_keeps_the_signed_hash_for_recovery() {
        use crate::{
            execute::PreparedArbTx, keypair::ExecutorKeypair, profit::ProfitBook, stats::ArbStats,
            submission_ledger::SubmissionLedger,
        };
        use soroban_client::{
            account::{Account, AccountBehavior},
            keypair::{Keypair, KeypairBehavior},
            transaction_builder::{TransactionBuilder, TransactionBuilderBehavior, TIMEOUT_INFINITE},
        };
        use std::sync::Arc;

        let key = ExecutorKeypair::new(Keypair::from_raw_ed25519_seed(&[7; 32]).unwrap());
        let mut account = Account::new(&key.public_key(), "1").unwrap();
        let mut builder = TransactionBuilder::new(&mut account, Networks::public(), None);
        builder.fee(100u32);
        let tx = builder.set_timeout(TIMEOUT_INFINITE).unwrap().build();
        let xdr = tx.to_envelope().unwrap().to_xdr_base64(Limits::none()).unwrap();
        let prepared = PreparedArbTx {
            base_token: "XLM".into(),
            route_label: "XLM → USDC → XLM".into(),
            venue_route_label: "venue".into(),
            pool_route_label: "pool".into(),
            caller_public_key: key.public_key(),
            amount_in: 100,
            quoted_amount_out: 120,
            simulated_amount_out: 110,
            estimated_fee_stroops: 2,
            fee_in_base: 2,
            xlm_price_e7: 3_000_000,
            profit_bps: 100,
            unsigned_tx_xdr: xdr,
            simulated: true,
        };
        let ledger = Arc::new(SubmissionLedger::open(":memory:").unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let stats = Arc::new(ArbStats::default());
        let error = super::submit_prepared(
            &url,
            &key,
            &prepared,
            stats.clone(),
            Arc::new(ProfitBook::default()),
            Some(ledger.clone()),
            false,
        )
        .await
        .unwrap_err();
        assert!(error.is::<super::BroadcastUncertain>());
        let pending = ledger.pending(i64::MAX, 10).unwrap();
        assert_eq!(pending.len(), 1);
        let (signed, _) = super::sign_transaction(&key, &prepared.unsigned_tx_xdr).unwrap();
        assert_eq!(pending[0].hash, hex::encode(signed.hash()));
        assert_eq!(stats.snapshot().txs_failed, 0);
        assert_eq!(stats.snapshot().txs_submitted, 0);
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

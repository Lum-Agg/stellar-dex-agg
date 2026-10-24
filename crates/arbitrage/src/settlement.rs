//! Chain-result accounting and bounded recovery of accepted submissions.

use crate::{
    prepare::rpc_server,
    profit::ProfitBook,
    runtime::SharedRuntime,
    stats::ArbStats,
    submission_ledger::{unix_seconds, PendingSubmission, SubmissionLedger},
};
use anyhow::Result;
use soroban_client::{
    soroban_rpc::{GetTransactionResponse, TransactionStatus},
    xdr::ScVal,
};
use std::{sync::atomic::Ordering, time::Duration};
use tracing::{info, warn};

#[derive(Debug)]
pub struct Settlement {
    pub success: bool,
    pub amount_out: Option<u128>,
    pub fee_stroops: Option<u128>,
    pub ledger: Option<u32>,
    pub reason: Option<String>,
}

impl Settlement {
    pub fn from_response(response: &GetTransactionResponse) -> Option<Self> {
        let success = match response.status {
            TransactionStatus::Success => true,
            TransactionStatus::Failed => false,
            _ => return None,
        };
        // TransactionResult.feeCharged is the final inclusion + resource fee,
        // not the simulated minimum or the envelope's maximum fee bid.
        let result = response.to_result();
        let fee_stroops = result.as_ref().and_then(|r| u128::try_from(r.fee_charged).ok());
        let amount_out = if success {
            response
                .to_result_meta()
                .and_then(|(_, value)| value)
                .and_then(|value| match value {
                    ScVal::I128(parts) => u128::try_from(((parts.hi as i128) << 64) | parts.lo as i128).ok(),
                    _ => None,
                })
        } else {
            None
        };
        let reason = if success {
            (amount_out.is_none() || fee_stroops.is_none()).then(|| "accounting_data_missing".to_owned())
        } else {
            Some(format!(
                "tx failed on-chain: {:?}; {}",
                result,
                crate::submit::decode_failure_diagnostics(response.to_diagnostic_events())
            ))
        };
        Some(Self {
            success,
            amount_out,
            fee_stroops,
            ledger: response.ledger,
            reason,
        })
    }
}

pub fn apply_settlement(
    tx: &PendingSubmission,
    outcome: &Settlement,
    ledger: Option<&SubmissionLedger>,
    stats: &ArbStats,
    profit: &ProfitBook,
) -> Result<bool> {
    if let Some(ledger) = ledger {
        if !ledger.finalize(
            &tx.hash,
            if outcome.success { "SUCCESS" } else { "FAILED" },
            outcome.amount_out,
            outcome.fee_stroops,
            outcome.ledger,
            outcome.reason.as_deref(),
        )? {
            return Ok(false);
        }
    }
    profit.record_settlement(tx, outcome);
    if outcome.success {
        stats.txs_succeeded.fetch_add(1, Ordering::Relaxed);
    } else {
        stats.txs_failed.fetch_add(1, Ordering::Relaxed);
        stats.record_failed_route(&tx.venue_route);
        if let Some(reason) = &outcome.reason {
            stats.record_chain_failure(reason);
        }
    }
    info!(hash = %tx.hash, route = %tx.route, success = outcome.success,
        actual_amount_out = ?outcome.amount_out, actual_fee_stroops = ?outcome.fee_stroops,
        simulated_amount_out = tx.simulated_amount_out, "arb transaction reconciled");
    Ok(true)
}

pub fn needs_history(tx: &PendingSubmission, response: &GetTransactionResponse) -> bool {
    // Use ledger close time, not local elapsed time: a lagging/unavailable RPC
    // cannot establish a history gap, let alone non-inclusion.
    response
        .oldest_ledger_close_time
        .parse::<i64>()
        .is_ok_and(|oldest| oldest > tx.submitted_at)
}

pub fn spawn_reconciler(runtime: SharedRuntime) {
    let Some(ledger) = runtime.submission_ledger.clone() else {
        return;
    };
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = reconcile_once(&runtime, &ledger).await {
                warn!(%error, "arb submission recovery failed");
            }
        }
    });
}

async fn reconcile_once(runtime: &SharedRuntime, ledger: &SubmissionLedger) -> Result<()> {
    reconcile_pending(&runtime.config.rpc_url, ledger, &runtime.stats, &runtime.profit).await
}

async fn reconcile_pending(
    rpc_url: &str,
    ledger: &SubmissionLedger,
    stats: &ArbStats,
    profit: &ProfitBook,
) -> Result<()> {
    let server = rpc_server(rpc_url)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    for tx in ledger.pending(unix_seconds(), 50)? {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        // Give the foreground poll its initial observation window.
        if unix_seconds() - tx.submitted_at < 35 {
            continue;
        }
        let request_deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(5));
        let response = tokio::time::timeout_at(request_deadline, server.get_transaction(&tx.hash)).await;
        match response {
            Ok(Ok(response)) => {
                if let Some(outcome) = Settlement::from_response(&response) {
                    apply_settlement(&tx, &outcome, Some(ledger), stats, profit)?;
                } else {
                    let history = needs_history(&tx, &response);
                    ledger.defer(
                        &tx.hash,
                        unix_seconds(),
                        history,
                        if history {
                            "outside_rpc_retention; historical verification required"
                        } else {
                            "not_found; not proof of failure"
                        },
                    )?;
                }
            }
            result => {
                ledger.defer(
                    &tx.hash,
                    unix_seconds(),
                    false,
                    &format!("RPC lookup error: {result:?}"),
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use soroban_client::xdr::{
        Int128Parts, Limits, SorobanTransactionMeta, SorobanTransactionMetaV2, TransactionMeta, TransactionMetaV3,
        TransactionMetaV4, TransactionResult, WriteXdr,
    };

    fn response_value(status: &str, version: u32, value: ScVal) -> Value {
        let meta = if version == 3 {
            TransactionMeta::V3(TransactionMetaV3 {
                soroban_meta: Some(SorobanTransactionMeta {
                    return_value: value,
                    ..Default::default()
                }),
                ..Default::default()
            })
        } else {
            TransactionMeta::V4(TransactionMetaV4 {
                soroban_meta: Some(SorobanTransactionMetaV2 {
                    return_value: Some(value),
                    ..Default::default()
                }),
                ..Default::default()
            })
        };
        let result = TransactionResult {
            fee_charged: 7,
            ..Default::default()
        };
        json!({"status": status, "latestLedger": 100, "latestLedgerCloseTime": "1000",
            "oldestLedger": 1, "oldestLedgerCloseTime": "1", "ledger": 99,
            "resultXdr": result.to_xdr_base64(Limits::none()).unwrap(),
            "resultMetaXdr": meta.to_xdr_base64(Limits::none()).unwrap()})
    }

    fn value(out: u64) -> ScVal {
        ScVal::I128(Int128Parts { hi: 0, lo: out })
    }

    #[test]
    fn decodes_actual_return_and_total_fee_for_both_meta_versions() {
        for version in [3, 4] {
            let response: GetTransactionResponse =
                serde_json::from_value(response_value("SUCCESS", version, value(105))).unwrap();
            let outcome = Settlement::from_response(&response).unwrap();
            assert!(outcome.success);
            assert_eq!(outcome.amount_out, Some(105));
            assert_eq!(outcome.fee_stroops, Some(7));
            assert_eq!(outcome.ledger, Some(99));
        }
    }

    #[test]
    fn failed_and_missing_metadata_do_not_invent_output() {
        let failed: GetTransactionResponse = serde_json::from_value(response_value("FAILED", 4, value(105))).unwrap();
        let outcome = Settlement::from_response(&failed).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.amount_out, None);
        assert_eq!(outcome.fee_stroops, Some(7));
        for invalid in [ScVal::Void, ScVal::I128(Int128Parts { hi: -1, lo: u64::MAX })] {
            let response = serde_json::from_value(response_value("SUCCESS", 4, invalid)).unwrap();
            let outcome = Settlement::from_response(&response).unwrap();
            assert_eq!(outcome.amount_out, None);
            assert_eq!(outcome.reason.as_deref(), Some("accounting_data_missing"));
        }
        let mut raw = response_value("SUCCESS", 4, value(100));
        raw.as_object_mut().unwrap().remove("resultMetaXdr");
        raw.as_object_mut().unwrap().remove("resultXdr");
        let outcome = Settlement::from_response(&serde_json::from_value(raw).unwrap()).unwrap();
        assert_eq!(outcome.amount_out, None);
        assert_eq!(outcome.fee_stroops, None);
    }

    async fn fake_rpc(result: Value) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let request = loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let len: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:").and_then(|v| v.trim().parse().ok()))
                        .unwrap();
                    if bytes.len() >= end + 4 + len {
                        break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + len]).unwrap();
                    }
                }
            };
            assert_eq!(request["method"], "getTransaction");
            let body = json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string();
            let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        format!("http://{address}")
    }

    fn pending_ledger() -> SubmissionLedger {
        let ledger = SubmissionLedger::open(":memory:").unwrap();
        ledger
            .record_submitted(
                "hash",
                "caller",
                "XLM → USDC → XLM",
                "venue",
                "pool",
                100,
                130,
                120,
                2,
                "XLM",
                0,
            )
            .unwrap();
        // Model a restart recovering an old persisted submission.
        ledger.defer("hash", 1, false, "old timeout").unwrap();
        ledger
    }

    #[tokio::test]
    async fn recovery_confirms_once_using_actual_not_simulated_profit() {
        let ledger = pending_ledger();
        let mut tx = ledger.pending(unix_seconds(), 1).unwrap().remove(0);
        // The worker normally waits 35s for the fast poll; set the fixture's
        // submission timestamp through a separate test helper below.
        age_submission(&ledger);
        let url = fake_rpc(response_value("SUCCESS", 4, value(105))).await;
        let stats = ArbStats::default();
        let profit = ProfitBook::default();
        reconcile_pending(&url, &ledger, &stats, &profit).await.unwrap();
        assert_eq!(ledger.unresolved_counts().unwrap(), (0, 0));
        tx.submitted_at = 1;
        let response = serde_json::from_value(response_value("SUCCESS", 4, value(105))).unwrap();
        assert!(!apply_settlement(
            &tx,
            &Settlement::from_response(&response).unwrap(),
            Some(&ledger),
            &stats,
            &profit
        )
        .unwrap());
        let (_, session, _) = profit.snapshot_for_hourly_report();
        assert_eq!(session["XLM"].succeeded, 1);
        assert_eq!(session["XLM"].gross_profit_stroops, 5);
        assert_eq!(session["XLM"].actual_fee_xlm_stroops, 7);
        assert_eq!(session["XLM"].net_profit_stroops(), -2);
    }

    fn age_submission(ledger: &SubmissionLedger) {
        ledger.age_test_submission();
    }

    #[tokio::test]
    async fn not_found_is_never_recorded_as_failed_or_expired() {
        for (oldest, expected_history) in [("0", 0), ("2", 1)] {
            let ledger = pending_ledger();
            age_submission(&ledger);
            let url = fake_rpc(json!({"status": "NOT_FOUND", "latestLedger": 100,
                "latestLedgerCloseTime": "1000", "oldestLedger": 1, "oldestLedgerCloseTime": oldest}))
            .await;
            let profit = ProfitBook::default();
            reconcile_pending(&url, &ledger, &ArbStats::default(), &profit)
                .await
                .unwrap();
            assert_eq!(ledger.unresolved_counts().unwrap(), (1, expected_history));
            assert!(profit.snapshot_for_hourly_report().1.is_empty());
        }
    }
}

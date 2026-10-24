//! Session / hourly profit accounting for Telegram reports.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

const RECENT_TXS: usize = 5;

#[derive(Debug, Default, Clone)]
pub struct ProfitWindow {
    pub succeeded: u64,
    pub failed: u64,
    /// Broadcast was accepted, but the short status poll did not reach a
    /// terminal result. This is unknown, not a failed transaction.
    pub unknown: u64,
    pub submitted: u64,
    /// Actual on-chain output minus principal, in base units.
    pub gross_profit_stroops: i128,
    /// Actual fees valued in base units at the submission-time XLM mark.
    pub fee_stroops: u128,
    pub actual_fee_xlm_stroops: u128,
    pub incomplete: u64,
    pub estimated_gross: i128,
    pub estimated_fee_xlm_stroops: u128,
}

impl ProfitWindow {
    pub fn net_profit_stroops(&self) -> i128 {
        self.gross_profit_stroops
            .saturating_sub(self.fee_stroops.min(i128::MAX as u128) as i128)
    }

    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

#[derive(Debug, Clone)]
pub struct RecentTx {
    pub hash: String,
    pub base_token: String,
    pub amount_in: u128,
    pub gross_profit: i128,
    pub fee: u128,
}

#[derive(Debug, Default)]
pub struct ProfitBook {
    inner: Mutex<ProfitBookInner>,
}

#[derive(Debug, Default)]
struct ProfitBookInner {
    session: BTreeMap<String, ProfitWindow>,
    hour: BTreeMap<String, ProfitWindow>,
    recent: VecDeque<RecentTx>,
}

impl ProfitBook {
    pub fn record_submitted(&self, base_token: &str) {
        let mut g = self.inner.lock().unwrap();
        g.session.entry(base_token.to_owned()).or_default().submitted += 1;
        g.hour.entry(base_token.to_owned()).or_default().submitted += 1;
    }

    pub fn record_settlement(
        &self,
        tx: &crate::submission_ledger::PendingSubmission,
        outcome: &crate::settlement::Settlement,
    ) {
        let base_token = &tx.base_token;
        let gross = if outcome.success {
            outcome.amount_out.and_then(|out| signed_delta(out, tx.amount_in))
        } else {
            None
        };
        let fee = outcome
            .fee_stroops
            .and_then(|fee| valued_fee(fee, base_token, tx.xlm_price_e7));
        let incomplete = outcome.fee_stroops.is_none() || fee.is_none() || (outcome.success && gross.is_none());
        let mut g = self.inner.lock().unwrap();
        let update = |w: &mut ProfitWindow| {
            if outcome.success {
                w.succeeded += 1;
            } else {
                w.failed += 1;
            }
            w.gross_profit_stroops = w.gross_profit_stroops.saturating_add(gross.unwrap_or(0));
            w.fee_stroops = w.fee_stroops.saturating_add(fee.unwrap_or(0));
            w.actual_fee_xlm_stroops = w
                .actual_fee_xlm_stroops
                .saturating_add(outcome.fee_stroops.unwrap_or(0));
            w.incomplete += u64::from(incomplete);
            w.estimated_gross = w
                .estimated_gross
                .saturating_add(signed_delta(tx.simulated_amount_out, tx.amount_in).unwrap_or(0));
            w.estimated_fee_xlm_stroops = w.estimated_fee_xlm_stroops.saturating_add(tx.estimated_fee_stroops);
        };
        update(g.session.entry(base_token.to_owned()).or_default());
        update(g.hour.entry(base_token.to_owned()).or_default());
        if outcome.success && !incomplete {
            g.recent.push_front(RecentTx {
                hash: tx.hash.clone(),
                base_token: base_token.to_owned(),
                amount_in: tx.amount_in,
                gross_profit: gross.unwrap(),
                fee: fee.unwrap(),
            });
        }
        while g.recent.len() > RECENT_TXS {
            g.recent.pop_back();
        }
    }

    pub fn record_unknown(&self, base_token: &str) {
        let mut g = self.inner.lock().unwrap();
        g.session.entry(base_token.to_owned()).or_default().unknown += 1;
        g.hour.entry(base_token.to_owned()).or_default().unknown += 1;
    }

    /// Snapshot session + take (reset) the hourly window for a report.
    pub fn snapshot_for_hourly_report(
        &self,
    ) -> (
        BTreeMap<String, ProfitWindow>,
        BTreeMap<String, ProfitWindow>,
        Vec<RecentTx>,
    ) {
        let mut g = self.inner.lock().unwrap();
        let hour = std::mem::take(&mut g.hour);
        let session = g.session.clone();
        let recent: Vec<_> = g.recent.iter().cloned().collect();
        (hour, session, recent)
    }
}

fn signed_delta(out: u128, input: u128) -> Option<i128> {
    i128::try_from(out).ok()?.checked_sub(i128::try_from(input).ok()?)
}

fn valued_fee(fee: u128, base: &str, price: u128) -> Option<u128> {
    use crate::economics::{UNIT_E7, USDC_SAC, XLM_SAC};
    match base {
        XLM_SAC | "XLM" => Some(fee),
        USDC_SAC | "USDC" if price > 0 => fee.checked_mul(price)?.checked_add(UNIT_E7 - 1).map(|n| n / UNIT_E7),
        _ => None,
    }
}

pub fn format_xlm4(stroops: i128) -> String {
    let neg = stroops < 0;
    let abs = stroops.unsigned_abs();
    let whole = abs / 10_000_000;
    let frac = abs % 10_000_000;
    // 4 decimal places
    let frac4 = frac / 1_000;
    if neg {
        format!("-{whole}.{frac4:04}")
    } else {
        format!("{whole}.{frac4:04}")
    }
}

pub fn format_xlm4_u(stroops: u128) -> String {
    format_xlm4(stroops as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_four_decimals() {
        assert_eq!(format_xlm4_u(595_035_000), "59.5035");
        assert_eq!(format_xlm4(1_074_562), "0.1074");
        assert_eq!(format_xlm4(-690_000), "-0.0690");
    }

    #[test]
    fn hour_window_resets() {
        let book = ProfitBook::default();
        let tx = crate::submission_ledger::PendingSubmission {
            hash: "abc".into(),
            submitted_at: 0,
            base_token: "XLM".into(),
            route: "".into(),
            venue_route: "".into(),
            amount_in: 100_000_000,
            simulated_amount_out: 100_380_000,
            estimated_fee_stroops: 1_074_562,
            xlm_price_e7: 0,
        };
        book.record_settlement(
            &tx,
            &crate::settlement::Settlement {
                success: true,
                amount_out: Some(100_200_000),
                fee_stroops: Some(100_000),
                ledger: Some(1),
                reason: None,
            },
        );
        let (hour, session, recent) = book.snapshot_for_hourly_report();
        assert_eq!(hour["XLM"].succeeded, 1);
        assert_eq!(session["XLM"].succeeded, 1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].base_token, "XLM");
        let (hour2, session2, _) = book.snapshot_for_hourly_report();
        assert!(hour2.is_empty());
        assert_eq!(session2["XLM"].succeeded, 1);
        assert_eq!(session2["XLM"].gross_profit_stroops, 200_000);
        assert_eq!(session2["XLM"].estimated_gross, 380_000);
        assert_eq!(session2["XLM"].actual_fee_xlm_stroops, 100_000);
    }

    #[test]
    fn failed_fees_and_signed_losses_are_included() {
        let tx = crate::submission_ledger::PendingSubmission {
            hash: "hash".into(),
            submitted_at: 0,
            base_token: "XLM".into(),
            route: "".into(),
            venue_route: "".into(),
            amount_in: 100,
            simulated_amount_out: 150,
            estimated_fee_stroops: 2,
            xlm_price_e7: 0,
        };
        let book = ProfitBook::default();
        book.record_settlement(
            &tx,
            &crate::settlement::Settlement {
                success: true,
                amount_out: Some(99),
                fee_stroops: Some(3),
                ledger: Some(1),
                reason: None,
            },
        );
        book.record_settlement(
            &tx,
            &crate::settlement::Settlement {
                success: false,
                amount_out: None,
                fee_stroops: Some(4),
                ledger: Some(2),
                reason: None,
            },
        );
        let (_, session, _) = book.snapshot_for_hourly_report();
        let w = &session["XLM"];
        assert_eq!(w.gross_profit_stroops, -1);
        assert_eq!(w.actual_fee_xlm_stroops, 7);
        assert_eq!(w.net_profit_stroops(), -8);
        assert_eq!(w.failed, 1);
        assert_eq!(w.incomplete, 0);
    }

    #[test]
    fn missing_metadata_never_uses_simulated_profit() {
        let tx = crate::submission_ledger::PendingSubmission {
            hash: "hash".into(),
            submitted_at: 0,
            base_token: "USDC".into(),
            route: "".into(),
            venue_route: "".into(),
            amount_in: 100,
            simulated_amount_out: 150,
            estimated_fee_stroops: 2,
            xlm_price_e7: 0,
        };
        let book = ProfitBook::default();
        book.record_settlement(
            &tx,
            &crate::settlement::Settlement {
                success: true,
                amount_out: None,
                fee_stroops: Some(3),
                ledger: Some(1),
                reason: None,
            },
        );
        let (_, session, recent) = book.snapshot_for_hourly_report();
        assert_eq!(session["USDC"].gross_profit_stroops, 0);
        assert_eq!(session["USDC"].incomplete, 1);
        assert_eq!(session["USDC"].actual_fee_xlm_stroops, 3);
        assert!(recent.is_empty());
        assert_eq!(valued_fee(115_155, "USDC", 3_000_000), Some(34_547));
        assert_eq!(valued_fee(100, "UNKNOWN_TOKEN", 3_000_000), None);
    }
}

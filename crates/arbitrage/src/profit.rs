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
    /// Sum of (simulated_amount_out - amount_in) on SUCCESS.
    pub gross_profit_stroops: u128,
    /// Sum of estimated (inclusion + resource) fees on SUCCESS.
    pub fee_stroops: u128,
}

impl ProfitWindow {
    pub fn net_profit_stroops(&self) -> i128 {
        self.gross_profit_stroops as i128 - self.fee_stroops as i128
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
    pub gross_profit: u128,
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

    pub fn record_success(&self, base_token: &str, hash: &str, amount_in: u128, gross_profit: u128, fee: u128) {
        let mut g = self.inner.lock().unwrap();
        let session = g.session.entry(base_token.to_owned()).or_default();
        session.succeeded += 1;
        session.gross_profit_stroops = session.gross_profit_stroops.saturating_add(gross_profit);
        session.fee_stroops = session.fee_stroops.saturating_add(fee);
        let hour = g.hour.entry(base_token.to_owned()).or_default();
        hour.succeeded += 1;
        hour.gross_profit_stroops = hour.gross_profit_stroops.saturating_add(gross_profit);
        hour.fee_stroops = hour.fee_stroops.saturating_add(fee);
        g.recent.push_front(RecentTx {
            hash: hash.to_string(),
            base_token: base_token.to_owned(),
            amount_in,
            gross_profit,
            fee,
        });
        while g.recent.len() > RECENT_TXS {
            g.recent.pop_back();
        }
    }

    pub fn record_failed(&self, base_token: &str) {
        let mut g = self.inner.lock().unwrap();
        g.session.entry(base_token.to_owned()).or_default().failed += 1;
        g.hour.entry(base_token.to_owned()).or_default().failed += 1;
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
        book.record_success("XLM", "abc", 100_000_000, 380_000, 1_074_562);
        let (hour, session, recent) = book.snapshot_for_hourly_report();
        assert_eq!(hour["XLM"].succeeded, 1);
        assert_eq!(session["XLM"].succeeded, 1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].base_token, "XLM");
        let (hour2, session2, _) = book.snapshot_for_hourly_report();
        assert!(hour2.is_empty());
        assert_eq!(session2["XLM"].succeeded, 1);
    }
}

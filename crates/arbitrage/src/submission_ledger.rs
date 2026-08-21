//! Durable record of transactions accepted by the Soroban RPC.

use {
    anyhow::{Context, Result},
    rusqlite::{params, Connection},
    std::{
        path::{Path, PathBuf},
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    },
};

#[derive(Debug)]
pub struct SubmissionLedger {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl SubmissionLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create submission ledger directory {}", parent.display()))?;
        }
        let connection =
            Connection::open(&path).with_context(|| format!("open submission ledger {}", path.display()))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS submissions (
                 tx_hash TEXT PRIMARY KEY,
                 submitted_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 caller TEXT NOT NULL,
                 route TEXT NOT NULL,
                 amount_in TEXT NOT NULL,
                 quoted_amount_out TEXT NOT NULL,
                 simulated_amount_out TEXT NOT NULL,
                 estimated_fee_stroops TEXT NOT NULL,
                 status TEXT NOT NULL,
                 failure_reason TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_submissions_status ON submissions(status);
             CREATE INDEX IF NOT EXISTS idx_submissions_submitted_at ON submissions(submitted_at);",
        )?;
        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_submitted(
        &self,
        tx_hash: &str,
        caller: &str,
        route: &str,
        amount_in: u128,
        quoted_amount_out: u128,
        simulated_amount_out: u128,
        estimated_fee_stroops: u128,
    ) -> Result<()> {
        let now = unix_seconds();
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        connection.execute(
            "INSERT INTO submissions
             (tx_hash, submitted_at, updated_at, caller, route, amount_in,
              quoted_amount_out, simulated_amount_out, estimated_fee_stroops, status)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'SUBMITTED')
             ON CONFLICT(tx_hash) DO UPDATE SET updated_at = excluded.updated_at",
            params![
                tx_hash,
                now,
                caller,
                route,
                amount_in.to_string(),
                quoted_amount_out.to_string(),
                simulated_amount_out.to_string(),
                estimated_fee_stroops.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn mark_status(&self, tx_hash: &str, status: &str, failure_reason: Option<&str>) -> Result<()> {
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        connection.execute(
            "UPDATE submissions SET updated_at = ?2, status = ?3, failure_reason = ?4 WHERE tx_hash = ?1",
            params![tx_hash, unix_seconds(), status, failure_reason],
        )?;
        Ok(())
    }
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::SubmissionLedger;

    #[test]
    fn persists_submission_and_final_status() {
        let path = std::env::temp_dir().join(format!("lumagg-ledger-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let ledger = SubmissionLedger::open(&path).unwrap();
        ledger
            .record_submitted("hash", "caller", "XLM->BLND->XLM", 10, 12, 11, 100)
            .unwrap();
        ledger.mark_status("hash", "FAILED", Some("min_amount_out")).unwrap();
        drop(ledger);

        let reopened = SubmissionLedger::open(&path).unwrap();
        let connection = reopened.connection.lock().unwrap();
        let row: (String, String, Option<String>) = connection
            .query_row(
                "SELECT status, amount_in, failure_reason FROM submissions WHERE tx_hash = 'hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "FAILED".to_string(),
                "10".to_string(),
                Some("min_amount_out".to_string())
            )
        );
        drop(connection);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}

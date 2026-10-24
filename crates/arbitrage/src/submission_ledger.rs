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

#[derive(Debug, Clone)]
pub struct PendingSubmission {
    pub hash: String,
    pub submitted_at: i64,
    pub base_token: String,
    pub route: String,
    pub venue_route: String,
    pub amount_in: u128,
    pub simulated_amount_out: u128,
    pub estimated_fee_stroops: u128,
    pub xlm_price_e7: u128,
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
                 venue_route TEXT NOT NULL DEFAULT '',
                 pool_route TEXT NOT NULL DEFAULT '',
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
        crate::diagnostics::initialize(&connection)?;
        // Keep existing production ledgers readable after adding venue-level
        // diagnostics. SQLite has no ADD COLUMN IF NOT EXISTS.
        let has_venue_route: bool = connection
            .prepare("PRAGMA table_info(submissions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .any(|column| column.map(|name| name == "venue_route").unwrap_or(false));
        if !has_venue_route {
            connection.execute(
                "ALTER TABLE submissions ADD COLUMN venue_route TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        let has_pool_route: bool = connection
            .prepare("PRAGMA table_info(submissions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .any(|column| column.map(|name| name == "pool_route").unwrap_or(false));
        if !has_pool_route {
            connection.execute(
                "ALTER TABLE submissions ADD COLUMN pool_route TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        for (name, definition) in [
            ("base_token", "TEXT NOT NULL DEFAULT ''"),
            ("xlm_price_e7", "TEXT NOT NULL DEFAULT '0'"),
            ("actual_amount_out", "TEXT"),
            ("actual_fee_stroops", "TEXT"),
            ("confirmed_ledger", "INTEGER"),
            ("next_check_at", "INTEGER NOT NULL DEFAULT 0"),
            ("check_attempts", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let exists = connection
                .prepare("PRAGMA table_info(submissions)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .any(|column| column.is_ok_and(|column| column == name));
            if !exists {
                connection.execute(&format!("ALTER TABLE submissions ADD COLUMN {name} {definition}"), [])?;
            }
        }
        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn persist_simulation_metrics(
        &self,
        rows: &[(crate::diagnostics::Dimension, crate::diagnostics::Aggregate)],
        now: i64,
        max_rows: usize,
    ) -> Result<()> {
        let mut connection = self.connection.lock().expect("submission ledger mutex poisoned");
        crate::diagnostics::persist(&mut connection, rows, now, max_rows)
    }

    pub fn record_submitted(
        &self,
        tx_hash: &str,
        caller: &str,
        route: &str,
        venue_route: &str,
        pool_route: &str,
        amount_in: u128,
        quoted_amount_out: u128,
        simulated_amount_out: u128,
        estimated_fee_stroops: u128,
        base_token: &str,
        xlm_price_e7: u128,
    ) -> Result<()> {
        let now = unix_seconds();
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        connection.execute(
            "INSERT INTO submissions
             (tx_hash, submitted_at, updated_at, caller, route, venue_route, pool_route, amount_in,
              quoted_amount_out, simulated_amount_out, estimated_fee_stroops, base_token, xlm_price_e7, status)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'SUBMITTED')
             ON CONFLICT(tx_hash) DO UPDATE SET updated_at = excluded.updated_at",
            params![
                tx_hash,
                now,
                caller,
                route,
                venue_route,
                pool_route,
                amount_in.to_string(),
                quoted_amount_out.to_string(),
                simulated_amount_out.to_string(),
                estimated_fee_stroops.to_string(),
                base_token,
                xlm_price_e7.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn mark_status(&self, tx_hash: &str, status: &str, failure_reason: Option<&str>) -> Result<()> {
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        connection.execute(
            "UPDATE submissions SET updated_at = ?2, status = ?3, failure_reason = ?4
             WHERE tx_hash = ?1 AND status IN ('SUBMITTED', 'UNKNOWN', 'HISTORY_REQUIRED')",
            params![tx_hash, unix_seconds(), status, failure_reason],
        )?;
        Ok(())
    }

    /// Bounded, fair restart recovery. Retry errors remain queryable; old RPC
    /// retention gaps are not evidence that a transaction failed or expired.
    pub fn pending(&self, now: i64, limit: usize) -> Result<Vec<PendingSubmission>> {
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        let mut stmt = connection.prepare(
            "SELECT tx_hash, submitted_at, base_token, route, venue_route, amount_in,
                    simulated_amount_out, estimated_fee_stroops, xlm_price_e7
             FROM submissions WHERE status IN ('SUBMITTED', 'UNKNOWN', 'HISTORY_REQUIRED')
             AND next_check_at <= ?1 ORDER BY next_check_at, submitted_at LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now, limit as i64], |row| {
            let number = |idx| -> rusqlite::Result<u128> {
                let value: String = row.get(idx)?;
                value.parse().map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(err))
                })
            };
            let route: String = row.get(3)?;
            let mut base_token: String = row.get(2)?;
            if base_token.is_empty() {
                base_token = route.split(" → ").next().unwrap_or("").to_owned();
            }
            Ok(PendingSubmission {
                hash: row.get(0)?,
                submitted_at: row.get(1)?,
                base_token,
                route,
                venue_route: row.get(4)?,
                amount_in: number(5)?,
                simulated_amount_out: number(6)?,
                estimated_fee_stroops: number(7)?,
                xlm_price_e7: number(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn defer(&self, hash: &str, now: i64, history_required: bool, reason: &str) -> Result<()> {
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        connection.execute(
            "UPDATE submissions SET updated_at = ?2, next_check_at = ?3,
             check_attempts = check_attempts + 1, status = ?4, failure_reason = ?5
             WHERE tx_hash = ?1 AND status IN ('SUBMITTED', 'UNKNOWN', 'HISTORY_REQUIRED')",
            params![
                hash,
                now,
                now + if history_required { 3600 } else { 30 },
                if history_required {
                    "HISTORY_REQUIRED"
                } else {
                    "UNKNOWN"
                },
                reason
            ],
        )?;
        Ok(())
    }

    /// Compare-and-set makes the fast poll and recovery worker idempotent.
    pub fn finalize(
        &self,
        hash: &str,
        status: &str,
        amount_out: Option<u128>,
        fee: Option<u128>,
        ledger: Option<u32>,
        reason: Option<&str>,
    ) -> Result<bool> {
        anyhow::ensure!(
            matches!(status, "SUCCESS" | "FAILED" | "REJECTED"),
            "invalid terminal status"
        );
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        let changed = connection.execute(
            "UPDATE submissions SET updated_at = ?2, status = ?3, actual_amount_out = ?4,
             actual_fee_stroops = ?5, confirmed_ledger = ?6, failure_reason = ?7
             WHERE tx_hash = ?1 AND status IN ('SUBMITTED', 'UNKNOWN', 'HISTORY_REQUIRED')",
            params![
                hash,
                unix_seconds(),
                status,
                amount_out.map(|n| n.to_string()),
                fee.map(|n| n.to_string()),
                ledger,
                reason
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn unresolved_counts(&self) -> Result<(u64, u64)> {
        let connection = self.connection.lock().expect("submission ledger mutex poisoned");
        Ok(connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(status = 'HISTORY_REQUIRED'), 0) FROM submissions
             WHERE status IN ('SUBMITTED', 'UNKNOWN', 'HISTORY_REQUIRED')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    #[cfg(test)]
    pub(crate) fn age_test_submission(&self) {
        self.connection
            .lock()
            .unwrap()
            .execute("UPDATE submissions SET submitted_at = 1", [])
            .unwrap();
    }
}

pub(crate) fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::SubmissionLedger;

    #[test]
    fn retries_are_fair_and_finalization_is_idempotent() {
        let ledger = SubmissionLedger::open(":memory:").unwrap();
        for hash in ["a", "b"] {
            ledger
                .record_submitted(
                    hash,
                    "caller",
                    "XLM → USDC → XLM",
                    "venue",
                    "pool",
                    100,
                    120,
                    110,
                    9,
                    "XLM",
                    0,
                )
                .unwrap();
        }
        let now = super::unix_seconds();
        assert_eq!(ledger.pending(now, 1).unwrap()[0].hash, "a");
        ledger.defer("a", now, false, "not found").unwrap();
        assert_eq!(ledger.pending(now, 1).unwrap()[0].hash, "b");
        ledger.defer("b", now, true, "history required").unwrap();
        assert_eq!(ledger.unresolved_counts().unwrap(), (2, 1));
        assert!(ledger.pending(now, 50).unwrap().is_empty());
        assert_eq!(ledger.pending(now + 30, 50).unwrap().len(), 1);
        assert!(ledger
            .finalize("a", "SUCCESS", Some(105), Some(3), Some(88), None)
            .unwrap());
        assert!(!ledger.finalize("a", "FAILED", None, Some(8), Some(89), None).unwrap());
        ledger.mark_status("a", "UNKNOWN", Some("late poll timeout")).unwrap();
        assert_eq!(ledger.unresolved_counts().unwrap(), (1, 1));
        let connection = ledger.connection.lock().unwrap();
        let actual: (String, String, String, u32) = connection.query_row(
            "SELECT status, actual_amount_out, actual_fee_stroops, confirmed_ledger FROM submissions WHERE tx_hash = 'a'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!(actual, ("SUCCESS".into(), "105".into(), "3".into(), 88));
    }

    #[test]
    fn old_schema_migrates_and_pending_survives_restart() {
        let path = std::env::temp_dir().join(format!(
            "lumagg-recovery-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE submissions (
            tx_hash TEXT PRIMARY KEY, submitted_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
            caller TEXT NOT NULL, route TEXT NOT NULL, amount_in TEXT NOT NULL,
            quoted_amount_out TEXT NOT NULL, simulated_amount_out TEXT NOT NULL,
            estimated_fee_stroops TEXT NOT NULL, status TEXT NOT NULL, failure_reason TEXT);
            INSERT INTO submissions VALUES ('old', 1, 1, 'caller', 'XLM → USDC → XLM', '100', '120', '110', '5', 'UNKNOWN', 'timeout');").unwrap();
        drop(connection);
        let ledger = SubmissionLedger::open(&path).unwrap();
        let pending = ledger.pending(10, 5).unwrap();
        assert_eq!(pending[0].base_token, "XLM");
        assert_eq!(pending[0].xlm_price_e7, 0);
        drop(ledger);
        let ledger = SubmissionLedger::open(&path).unwrap();
        assert_eq!(ledger.pending(10, 5).unwrap().len(), 1);
        drop(ledger);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_submission_and_final_status() {
        let path = std::env::temp_dir().join(format!("lumagg-ledger-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let ledger = SubmissionLedger::open(&path).unwrap();
        ledger
            .record_submitted(
                "hash",
                "caller",
                "XLM->BLND->XLM",
                "aquarius → aquarius_clmm",
                "aquarius:POOL-A → aquarius_clmm:POOL-B || aquarius_clmm:POOL-C",
                10,
                12,
                11,
                100,
                "XLM",
                0,
            )
            .unwrap();
        ledger.mark_status("hash", "FAILED", Some("min_amount_out")).unwrap();
        drop(ledger);

        let reopened = SubmissionLedger::open(&path).unwrap();
        let connection = reopened.connection.lock().unwrap();
        let row: (String, String, String, String, Option<String>) = connection
            .query_row(
                "SELECT status, amount_in, venue_route, pool_route, failure_reason FROM submissions WHERE tx_hash = 'hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "FAILED".to_string(),
                "10".to_string(),
                "aquarius → aquarius_clmm".to_string(),
                "aquarius:POOL-A → aquarius_clmm:POOL-B || aquarius_clmm:POOL-C".to_string(),
                Some("min_amount_out".to_string())
            )
        );
        drop(connection);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}

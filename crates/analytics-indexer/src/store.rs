//! SQLite persistence for indexed aggregator invocations.

use {
    crate::parser::{ParsedInvocation, ParsedLeg},
    anyhow::{Context, Result},
    rusqlite::{params, Connection},
    std::path::Path,
};

#[derive(Debug, Clone)]
pub struct StoredInvocation {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub parsed: ParsedInvocation,
}

pub struct IndexStore {
    conn: Connection,
}

impl IndexStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("open sqlite db")?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS indexer_cursor (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                last_ledger INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS swap_invocations (
                tx_hash TEXT PRIMARY KEY,
                ledger INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                function_name TEXT NOT NULL,
                user_address TEXT NOT NULL,
                token_in TEXT,
                token_out TEXT,
                amount_in TEXT NOT NULL,
                amount_out TEXT,
                is_split INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS swap_legs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tx_hash TEXT NOT NULL,
                leg_index INTEGER NOT NULL,
                dex_source TEXT NOT NULL,
                pool_address TEXT NOT NULL,
                token_in TEXT,
                token_out TEXT,
                amount_in TEXT,
                FOREIGN KEY (tx_hash) REFERENCES swap_invocations(tx_hash)
            );

            CREATE INDEX IF NOT EXISTS idx_swap_invocations_ledger ON swap_invocations(ledger);
            CREATE INDEX IF NOT EXISTS idx_swap_invocations_created ON swap_invocations(created_at);
            CREATE INDEX IF NOT EXISTS idx_swap_legs_dex ON swap_legs(dex_source);
            ",
        )?;
        Ok(())
    }

    pub fn cursor_ledger(&self) -> Result<Option<u32>> {
        let mut stmt = self
            .conn
            .prepare("SELECT last_ledger FROM indexer_cursor WHERE id = 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let ledger: i64 = row.get(0)?;
            Ok(Some(ledger as u32))
        } else {
            Ok(None)
        }
    }

    pub fn set_cursor_ledger(&self, ledger: u32) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO indexer_cursor (id, last_ledger, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET last_ledger = excluded.last_ledger, updated_at = excluded.updated_at",
            params![ledger, now],
        )?;
        Ok(())
    }

    pub fn insert_invocation(&self, record: &StoredInvocation) -> Result<bool> {
        let p = &record.parsed;
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO swap_invocations (
                tx_hash, ledger, created_at, status, function_name, user_address,
                token_in, token_out, amount_in, amount_out, is_split
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.tx_hash,
                record.ledger,
                record.created_at,
                record.status,
                p.function_name,
                p.user_address,
                p.token_in,
                p.token_out,
                p.amount_in.to_string(),
                p.amount_out.map(|v| v.to_string()),
                p.is_split as i32,
            ],
        )?;

        if inserted == 0 {
            return Ok(false);
        }

        for leg in &p.legs {
            self.insert_leg(&record.tx_hash, leg)?;
        }
        Ok(true)
    }

    fn insert_leg(&self, tx_hash: &str, leg: &ParsedLeg) -> Result<()> {
        self.conn.execute(
            "INSERT INTO swap_legs (
                tx_hash, leg_index, dex_source, pool_address, token_in, token_out, amount_in
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                tx_hash,
                leg.leg_index,
                leg.dex_source,
                leg.pool_address,
                leg.token_in,
                leg.token_out,
                leg.amount_in.map(|v| v.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn count_invocations(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM swap_invocations", [], |r| r.get(0))?;
        Ok(count)
    }

    pub fn oldest_created_at(&self) -> Result<Option<i64>> {
        let ts: Option<i64> = self
            .conn
            .query_row("SELECT MIN(created_at) FROM swap_invocations", [], |r| r.get(0))?;
        Ok(ts)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

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
    pub failure_reason: Option<String>,
    pub parsed: ParsedInvocation,
}

#[derive(Debug, Clone)]
pub struct LimitOrderRow {
    pub escrow_contract: String,
    pub order_id: i64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_initial: Option<String>,
    pub amount_in_remaining: String,
    pub limit_out_per_in_e7: String,
    pub expires_ledger: u32,
    pub status: String,
    pub created_ledger: Option<u32>,
    pub updated_ledger: u32,
    pub created_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DcaOrderRow {
    pub escrow_contract: String,
    pub order_id: i64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_initial: String,
    pub amount_in_remaining: String,
    pub chunk_amount: String,
    pub interval_ledgers: u32,
    pub next_executable_ledger: u32,
    pub min_out_per_in_e7: String,
    pub expires_ledger: u32,
    pub status: String,
    pub updated_ledger: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct UserSwapRow {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub function_name: String,
    pub user_address: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub is_split: bool,
}

#[derive(Debug, Clone)]
pub struct RoundTripRow {
    pub tx_hash: String,
    pub ledger: u32,
    pub created_at: i64,
    pub status: String,
    pub user_address: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub bridge_token: Option<String>,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub is_split: bool,
}

pub struct IndexStore {
    conn: Connection,
}

fn map_limit_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LimitOrderRow> {
    Ok(LimitOrderRow {
        escrow_contract: row.get(0)?,
        order_id: row.get(1)?,
        owner: row.get(2)?,
        token_in: row.get(3)?,
        token_out: row.get(4)?,
        amount_in_initial: row.get(5)?,
        amount_in_remaining: row.get(6)?,
        limit_out_per_in_e7: row.get(7)?,
        expires_ledger: row.get::<_, i64>(8)? as u32,
        status: row.get(9)?,
        created_ledger: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
        updated_ledger: row.get::<_, i64>(11)? as u32,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_dca_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DcaOrderRow> {
    Ok(DcaOrderRow {
        escrow_contract: row.get(0)?,
        order_id: row.get(1)?,
        owner: row.get(2)?,
        token_in: row.get(3)?,
        token_out: row.get(4)?,
        amount_in_initial: row.get(5)?,
        amount_in_remaining: row.get(6)?,
        chunk_amount: row.get(7)?,
        interval_ledgers: row.get::<_, i64>(8)? as u32,
        next_executable_ledger: row.get::<_, i64>(9)? as u32,
        min_out_per_in_e7: row.get(10)?,
        expires_ledger: row.get::<_, i64>(11)? as u32,
        status: row.get(12)?,
        updated_ledger: row.get::<_, i64>(13)? as u32,
        updated_at: row.get(14)?,
    })
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
                failure_reason TEXT,
                function_name TEXT NOT NULL,
                user_address TEXT NOT NULL,
                token_in TEXT,
                token_out TEXT,
                bridge_token TEXT,
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
                amount_out TEXT,
                amount_is_actual INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (tx_hash) REFERENCES swap_invocations(tx_hash)
            );

            CREATE INDEX IF NOT EXISTS idx_swap_invocations_ledger ON swap_invocations(ledger);
            CREATE INDEX IF NOT EXISTS idx_swap_invocations_created ON swap_invocations(created_at);
            CREATE INDEX IF NOT EXISTS idx_swap_invocations_user_created
              ON swap_invocations(user_address, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_swap_legs_dex ON swap_legs(dex_source);

            CREATE TABLE IF NOT EXISTS limit_orders (
              escrow_contract TEXT NOT NULL,
              order_id INTEGER NOT NULL,
              owner TEXT NOT NULL,
              token_in TEXT NOT NULL,
              token_out TEXT NOT NULL,
              amount_in_initial TEXT,
              amount_in_remaining TEXT NOT NULL,
              limit_out_per_in_e7 TEXT NOT NULL,
              expires_ledger INTEGER NOT NULL,
              status TEXT NOT NULL,
              created_ledger INTEGER,
              updated_ledger INTEGER NOT NULL,
              created_at INTEGER,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (escrow_contract, order_id)
            );

            CREATE INDEX IF NOT EXISTS idx_limit_orders_owner ON limit_orders(owner, status);

            CREATE TABLE IF NOT EXISTS dca_orders (
              escrow_contract TEXT NOT NULL,
              order_id INTEGER NOT NULL,
              owner TEXT NOT NULL,
              token_in TEXT NOT NULL,
              token_out TEXT NOT NULL,
              amount_in_initial TEXT NOT NULL,
              amount_in_remaining TEXT NOT NULL,
              chunk_amount TEXT NOT NULL,
              interval_ledgers INTEGER NOT NULL,
              next_executable_ledger INTEGER NOT NULL,
              min_out_per_in_e7 TEXT NOT NULL,
              expires_ledger INTEGER NOT NULL,
              status TEXT NOT NULL,
              updated_ledger INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (escrow_contract, order_id)
            );

            CREATE INDEX IF NOT EXISTS idx_dca_orders_owner ON dca_orders(owner, status);
            ",
        )?;
        // Older production DBs were created before bridge_token / amount_is_actual
        // existed. CREATE TABLE IF NOT EXISTS does not alter existing tables.
        self.ensure_column("swap_invocations", "bridge_token", "TEXT")?;
        self.ensure_column("swap_invocations", "failure_reason", "TEXT")?;
        self.ensure_column("swap_invocations", "is_split", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("swap_legs", "token_in", "TEXT")?;
        self.ensure_column("swap_legs", "token_out", "TEXT")?;
        self.ensure_column("swap_legs", "amount_out", "TEXT")?;
        // Legacy rows predate the actual-vs-envelope distinction; treat them as
        // actual so historical routed volume does not collapse to zero.
        self.ensure_column("swap_legs", "amount_is_actual", "INTEGER NOT NULL DEFAULT 1")?;
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .with_context(|| format!("pragma table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_column(&self, table: &str, column: &str, ddl_type: &str) -> Result<()> {
        if self.table_has_column(table, column)? {
            return Ok(());
        }
        self.conn
            .execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl_type}"), [])
            .with_context(|| format!("add column {table}.{column}"))?;
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
                token_in, token_out, bridge_token, amount_in, amount_out, is_split, failure_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.tx_hash,
                record.ledger,
                record.created_at,
                record.status,
                p.function_name,
                p.user_address,
                p.token_in,
                p.token_out,
                p.bridge_token,
                p.amount_in.to_string(),
                p.amount_out.map(|v| v.to_string()),
                p.is_split as i32,
                record.failure_reason,
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

    /// Enrich event-derived legs with envelope route metadata. Match by hop,
    /// DEX, and pool so actual event amounts stay attached to the correct leg.
    pub fn replace_invocation_legs(&self, tx_hash: &str, parsed: &crate::parser::ParsedInvocation) -> Result<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM swap_invocations WHERE tx_hash = ?1",
            params![tx_hash],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }

        let mut stored_legs = {
            let mut stmt = self.conn.prepare(
                "SELECT leg_index, dex_source, pool_address, amount_in, amount_out,
                        amount_is_actual
                 FROM swap_legs WHERE tx_hash = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map(params![tx_hash], |row| {
                Ok((
                    row.get::<_, i64>(0)? as u32,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?
                        .and_then(|value| value.parse::<i128>().ok()),
                    row.get::<_, Option<String>>(4)?
                        .and_then(|value| value.parse::<i128>().ok()),
                    row.get::<_, i32>(5)? != 0,
                ))
            })?;
            let mut legs = Vec::new();
            for row in rows {
                legs.push(row?);
            }
            legs
        };

        self.conn
            .execute("DELETE FROM swap_legs WHERE tx_hash = ?1", params![tx_hash])?;
        self.conn.execute(
            "UPDATE swap_invocations
             SET bridge_token = ?1,
                 is_split = ?2,
                 amount_out = COALESCE(amount_out, ?3)
             WHERE tx_hash = ?4",
            params![
                parsed.bridge_token,
                parsed.is_split as i32,
                parsed.amount_out.map(|v| v.to_string()),
                tx_hash
            ],
        )?;
        for leg in &parsed.legs {
            let mut enriched = leg.clone();
            if let Some(index) = stored_legs.iter().position(|stored| {
                stored.0 == leg.leg_index && stored.1 == leg.dex_source && stored.2 == leg.pool_address
            }) {
                let stored = stored_legs.remove(index);
                enriched.amount_in = stored.3.or(leg.amount_in);
                enriched.amount_out = stored.4.or(leg.amount_out);
                enriched.amount_is_actual = stored.5;
            }
            self.insert_leg(tx_hash, &enriched)?;
        }
        Ok(true)
    }

    pub fn update_invocation_status(&self, tx_hash: &str, status: &str) -> Result<bool> {
        // A pruned Soroban RPC reports historical transactions as NOT_FOUND.
        // That is an availability result, not a terminal chain status, and
        // must never overwrite a previously indexed SUCCESS or FAILED row.
        if status == "NOT_FOUND" {
            return Ok(false);
        }
        Ok(self.conn.execute(
            "UPDATE swap_invocations SET status = ?1 WHERE tx_hash = ?2",
            params![status, tx_hash],
        )? > 0)
    }

    pub fn update_invocation_failure_reason(&self, tx_hash: &str, reason: Option<&str>) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE swap_invocations SET failure_reason = COALESCE(failure_reason, ?1)
             WHERE tx_hash = ?2 AND status = 'FAILED'",
            params![reason, tx_hash],
        )? > 0)
    }

    fn insert_leg(&self, tx_hash: &str, leg: &ParsedLeg) -> Result<()> {
        self.conn.execute(
            "INSERT INTO swap_legs (
                tx_hash, leg_index, dex_source, pool_address, token_in, token_out,
                amount_in, amount_out, amount_is_actual
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                tx_hash,
                leg.leg_index,
                leg.dex_source,
                leg.pool_address,
                leg.token_in,
                leg.token_out,
                leg.amount_in.map(|v| v.to_string()),
                leg.amount_out.map(|v| v.to_string()),
                leg.amount_is_actual as i32,
            ],
        )?;
        Ok(())
    }

    pub fn list_tx_hashes_since(&self, created_at_from: i64) -> Result<Vec<(String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tx_hash, ledger FROM swap_invocations
             WHERE created_at >= ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![created_at_from], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_unclassified_failed_tx_hashes(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT tx_hash FROM swap_invocations
             WHERE function_name = 'round_trip_swap'
               AND status = 'FAILED' AND failure_reason IS NULL
             ORDER BY created_at ASC, tx_hash ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
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

    /// List swaps for `user`, newest first.
    ///
    /// When `before` is `Some((created_at, tx_hash))`, returns rows strictly
    /// older than that cursor (`ORDER BY created_at DESC, tx_hash DESC`).
    pub fn list_swaps_by_user(&self, user: &str, limit: u32, before: Option<(i64, &str)>) -> Result<Vec<UserSwapRow>> {
        let limit = limit.clamp(1, 50);
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<UserSwapRow> {
            Ok(UserSwapRow {
                tx_hash: row.get(0)?,
                ledger: row.get::<_, i64>(1)? as u32,
                created_at: row.get(2)?,
                status: row.get(3)?,
                function_name: row.get(4)?,
                user_address: row.get(5)?,
                token_in: row.get(6)?,
                token_out: row.get(7)?,
                amount_in: row.get(8)?,
                amount_out: row.get(9)?,
                is_split: row.get::<_, i32>(10)? != 0,
            })
        };

        let mut out = Vec::new();
        if let Some((created_at, tx_hash)) = before {
            let mut stmt = self.conn.prepare(
                "SELECT tx_hash, ledger, created_at, status, function_name, user_address,
                        token_in, token_out, amount_in, amount_out, is_split
                 FROM swap_invocations
                 WHERE user_address = ?1
                   AND (created_at < ?2 OR (created_at = ?2 AND tx_hash < ?3))
                 ORDER BY created_at DESC, tx_hash DESC
                 LIMIT ?4",
            )?;
            let rows = stmt.query_map(params![user, created_at, tx_hash, limit], map_row)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT tx_hash, ledger, created_at, status, function_name, user_address,
                        token_in, token_out, amount_in, amount_out, is_split
                 FROM swap_invocations
                 WHERE user_address = ?1
                 ORDER BY created_at DESC, tx_hash DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![user, limit], map_row)?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    /// Successful on-chain `round_trip_swap` rows, newest first.
    ///
    /// When `before` is `Some((created_at, tx_hash))`, returns rows strictly
    /// older than that cursor (`ORDER BY created_at DESC, tx_hash DESC`).
    pub fn list_recent_round_trips(&self, limit: u32, before: Option<(i64, &str)>) -> Result<Vec<RoundTripRow>> {
        let limit = limit.clamp(1, 50);
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<RoundTripRow> {
            Ok(RoundTripRow {
                tx_hash: row.get(0)?,
                ledger: row.get::<_, i64>(1)? as u32,
                created_at: row.get(2)?,
                status: row.get(3)?,
                user_address: row.get(4)?,
                token_in: row.get(5)?,
                token_out: row.get(6)?,
                bridge_token: row.get(7)?,
                amount_in: row.get(8)?,
                amount_out: row.get(9)?,
                is_split: row.get::<_, i32>(10)? != 0,
            })
        };

        let mut out = Vec::new();
        if let Some((created_at, tx_hash)) = before {
            let mut stmt = self.conn.prepare(
                "SELECT tx_hash, ledger, created_at, status, user_address,
                        token_in, token_out, bridge_token, amount_in, amount_out, is_split
                 FROM swap_invocations
                 WHERE function_name = 'round_trip_swap'
                   AND status = 'SUCCESS'
                   AND (created_at < ?1 OR (created_at = ?1 AND tx_hash < ?2))
                 ORDER BY created_at DESC, tx_hash DESC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![created_at, tx_hash, limit], map_row)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT tx_hash, ledger, created_at, status, user_address,
                        token_in, token_out, bridge_token, amount_in, amount_out, is_split
                 FROM swap_invocations
                 WHERE function_name = 'round_trip_swap'
                   AND status = 'SUCCESS'
                 ORDER BY created_at DESC, tx_hash DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_row)?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    /// Count indexed round-trip invocations by terminal on-chain status.
    pub fn round_trip_status_counts(&self) -> Result<(u64, u64)> {
        let mut stmt = self.conn.prepare(
            "SELECT status, COUNT(*) FROM swap_invocations
             WHERE function_name = 'round_trip_swap' AND status IN ('SUCCESS', 'FAILED')
             GROUP BY status",
        )?;
        let mut success = 0u64;
        let mut failed = 0u64;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64)))?;
        for row in rows {
            match row? {
                (status, count) if status == "SUCCESS" => success = count,
                (status, count) if status == "FAILED" => failed = count,
                _ => {}
            }
        }
        Ok((success, failed))
    }

    /// Count classified on-chain failures. Legacy failures without a decoded
    /// result remain excluded rather than being assigned a guessed reason.
    pub fn round_trip_failure_reason_counts(&self) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT failure_reason, COUNT(*) FROM swap_invocations
             WHERE function_name = 'round_trip_swap' AND status = 'FAILED'
               AND failure_reason IS NOT NULL
             GROUP BY failure_reason ORDER BY COUNT(*) DESC, failure_reason ASC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn upsert_created(
        &self,
        order_id: i64,
        owner: &str,
        token_in: &str,
        token_out: &str,
        amount_in_initial: &str,
        amount_in_remaining: &str,
        limit_out_per_in_e7: &str,
        expires_ledger: u32,
        created_ledger: u32,
        updated_ledger: u32,
        created_at: i64,
        updated_at: i64,
    ) -> Result<()> {
        self.upsert_created_for(
            "",
            order_id,
            owner,
            token_in,
            token_out,
            amount_in_initial,
            amount_in_remaining,
            limit_out_per_in_e7,
            expires_ledger,
            created_ledger,
            updated_ledger,
            created_at,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_created_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        owner: &str,
        token_in: &str,
        token_out: &str,
        amount_in_initial: &str,
        amount_in_remaining: &str,
        limit_out_per_in_e7: &str,
        expires_ledger: u32,
        created_ledger: u32,
        updated_ledger: u32,
        created_at: i64,
        updated_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO limit_orders (
                escrow_contract, order_id, owner, token_in, token_out, amount_in_initial, amount_in_remaining,
                limit_out_per_in_e7, expires_ledger, status, created_ledger, updated_ledger,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'open', ?10, ?11, ?12, ?13)",
            params![
                escrow_contract,
                order_id,
                owner,
                token_in,
                token_out,
                amount_in_initial,
                amount_in_remaining,
                limit_out_per_in_e7,
                expires_ledger,
                created_ledger,
                updated_ledger,
                created_at,
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// Returns `true` when the row was updated, `false` when the order is
    /// missing or already in a terminal state.
    pub fn apply_filled(
        &self,
        order_id: i64,
        amount_in_remaining: &str,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        self.apply_filled_for("", order_id, amount_in_remaining, updated_ledger, updated_at)
    }

    pub fn apply_filled_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        amount_in_remaining: &str,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        let status = if amount_in_remaining == "0" { "filled" } else { "open" };
        let updated = self.conn.execute(
            "UPDATE limit_orders
             SET amount_in_remaining = ?1, status = ?2, updated_ledger = ?3, updated_at = ?4
             WHERE escrow_contract = ?5 AND order_id = ?6 AND status = 'open'",
            params![
                amount_in_remaining,
                status,
                updated_ledger,
                updated_at,
                escrow_contract,
                order_id,
            ],
        )?;
        Ok(updated > 0)
    }

    /// Returns `true` when the row was updated, `false` when the order is
    /// missing or already in a terminal state.
    pub fn apply_closed(&self, order_id: i64, status: &str, updated_ledger: u32, updated_at: i64) -> Result<bool> {
        anyhow::ensure!(
            status == "cancelled" || status == "expired",
            "invalid closed status: {status}"
        );
        self.apply_closed_for("", order_id, status, updated_ledger, updated_at)
    }

    pub fn apply_closed_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        status: &str,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE limit_orders
             SET status = ?1, amount_in_remaining = '0', updated_ledger = ?2, updated_at = ?3
             WHERE escrow_contract = ?4 AND order_id = ?5 AND status = 'open'",
            params![status, updated_ledger, updated_at, escrow_contract, order_id],
        )?;
        Ok(updated > 0)
    }

    pub fn list_by_owner(&self, owner: &str, status_filter: Option<&str>) -> Result<Vec<LimitOrderRow>> {
        self.list_by_owner_for("", owner, status_filter)
    }

    pub fn list_by_owner_for(
        &self,
        escrow_contract: &str,
        owner: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<LimitOrderRow>> {
        let sql = match status_filter {
            Some("all") => {
                "SELECT escrow_contract, order_id, owner, token_in, token_out, amount_in_initial,
                        amount_in_remaining, limit_out_per_in_e7, expires_ledger, status,
                        created_ledger, updated_ledger, created_at, updated_at
                 FROM limit_orders
                 WHERE escrow_contract = ?1 AND owner = ?2
                 ORDER BY updated_at DESC, order_id DESC"
            }
            _ => {
                "SELECT escrow_contract, order_id, owner, token_in, token_out, amount_in_initial,
                        amount_in_remaining, limit_out_per_in_e7, expires_ledger, status,
                        created_ledger, updated_ledger, created_at, updated_at
                 FROM limit_orders
                 WHERE escrow_contract = ?1 AND owner = ?2 AND status = 'open'
                 ORDER BY updated_at DESC, order_id DESC"
            }
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![escrow_contract, owner], map_limit_order_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_dca_created(
        &self,
        order_id: i64,
        owner: &str,
        token_in: &str,
        token_out: &str,
        amount_in: &str,
        chunk_amount: &str,
        interval_ledgers: u32,
        next_executable_ledger: u32,
        min_out_per_in_e7: &str,
        expires_ledger: u32,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<()> {
        self.upsert_dca_created_for(
            "",
            order_id,
            owner,
            token_in,
            token_out,
            amount_in,
            chunk_amount,
            interval_ledgers,
            next_executable_ledger,
            min_out_per_in_e7,
            expires_ledger,
            updated_ledger,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_dca_created_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        owner: &str,
        token_in: &str,
        token_out: &str,
        amount_in: &str,
        chunk_amount: &str,
        interval_ledgers: u32,
        next_executable_ledger: u32,
        min_out_per_in_e7: &str,
        expires_ledger: u32,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO dca_orders (
              escrow_contract, order_id, owner, token_in, token_out, amount_in_initial, amount_in_remaining,
              chunk_amount, interval_ledgers, next_executable_ledger, min_out_per_in_e7,
              expires_ledger, status, updated_ledger, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11, 'open', ?12, ?13)",
            params![
                escrow_contract,
                order_id,
                owner,
                token_in,
                token_out,
                amount_in,
                chunk_amount,
                interval_ledgers,
                next_executable_ledger,
                min_out_per_in_e7,
                expires_ledger,
                updated_ledger,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn apply_dca_filled(
        &self,
        order_id: i64,
        amount_in_remaining: &str,
        next_executable_ledger: u32,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        self.apply_dca_filled_for(
            "",
            order_id,
            amount_in_remaining,
            next_executable_ledger,
            updated_ledger,
            updated_at,
        )
    }

    pub fn apply_dca_filled_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        amount_in_remaining: &str,
        next_executable_ledger: u32,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        let status = if amount_in_remaining == "0" { "filled" } else { "open" };
        Ok(self.conn.execute(
            "UPDATE dca_orders SET amount_in_remaining=?1, next_executable_ledger=?2,
             status=?3, updated_ledger=?4, updated_at=?5 WHERE escrow_contract=?6 AND order_id=?7 AND status='open'",
            params![
                amount_in_remaining,
                next_executable_ledger,
                status,
                updated_ledger,
                updated_at,
                escrow_contract,
                order_id
            ],
        )? > 0)
    }

    pub fn apply_dca_closed(&self, order_id: i64, status: &str, updated_ledger: u32, updated_at: i64) -> Result<bool> {
        self.apply_dca_closed_for("", order_id, status, updated_ledger, updated_at)
    }

    pub fn apply_dca_closed_for(
        &self,
        escrow_contract: &str,
        order_id: i64,
        status: &str,
        updated_ledger: u32,
        updated_at: i64,
    ) -> Result<bool> {
        anyhow::ensure!(status == "cancelled" || status == "expired", "invalid DCA status");
        Ok(self.conn.execute(
            "UPDATE dca_orders SET amount_in_remaining='0', status=?1, updated_ledger=?2,
             updated_at=?3 WHERE escrow_contract=?4 AND order_id=?5 AND status='open'",
            params![status, updated_ledger, updated_at, escrow_contract, order_id],
        )? > 0)
    }

    pub fn list_dca_by_owner(&self, owner: &str, include_all: bool) -> Result<Vec<DcaOrderRow>> {
        self.list_dca_by_owner_for("", owner, include_all)
    }

    pub fn list_dca_by_owner_for(
        &self,
        escrow_contract: &str,
        owner: &str,
        include_all: bool,
    ) -> Result<Vec<DcaOrderRow>> {
        let sql = if include_all {
            "SELECT escrow_contract,order_id,owner,token_in,token_out,amount_in_initial,amount_in_remaining,
             chunk_amount,interval_ledgers,next_executable_ledger,min_out_per_in_e7,expires_ledger,
             status,updated_ledger,updated_at FROM dca_orders WHERE escrow_contract=?1 AND owner=?2 ORDER BY updated_at DESC"
        } else {
            "SELECT escrow_contract,order_id,owner,token_in,token_out,amount_in_initial,amount_in_remaining,
             chunk_amount,interval_ledgers,next_executable_ledger,min_out_per_in_e7,expires_ledger,
             status,updated_ledger,updated_at FROM dca_orders WHERE escrow_contract=?1 AND owner=?2 AND status='open' ORDER BY updated_at DESC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![escrow_contract, owner], map_dca_order_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::parser::{ParsedInvocation, ParsedLeg},
        tempfile::tempdir,
    };

    fn sample(tx_hash: &str, user: &str, created_at: i64, amount_in: i128) -> StoredInvocation {
        StoredInvocation {
            tx_hash: tx_hash.into(),
            ledger: 1,
            created_at,
            status: "SUCCESS".into(),
            failure_reason: None,
            parsed: ParsedInvocation {
                function_name: "swap".into(),
                user_address: user.into(),
                token_in: Some("TOKEN_IN".into()),
                token_out: Some("TOKEN_OUT".into()),
                bridge_token: None,
                amount_in,
                amount_out: Some(amount_in + 1),
                is_split: false,
                legs: vec![ParsedLeg {
                    leg_index: 0,
                    dex_source: "soroswap".into(),
                    pool_address: "POOL".into(),
                    token_in: Some("TOKEN_IN".into()),
                    token_out: Some("TOKEN_OUT".into()),
                    amount_in: Some(amount_in),
                    amount_out: None,
                    amount_is_actual: false,
                }],
            },
        }
    }

    #[test]
    fn list_swaps_by_user_filters_orders_and_limits() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = IndexStore::open(&path).unwrap();
        let u1 = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let u2 = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        store.insert_invocation(&sample("tx_old", u1, 100, 10)).unwrap();
        store.insert_invocation(&sample("tx_new", u1, 200, 20)).unwrap();
        store.insert_invocation(&sample("tx_other", u2, 300, 30)).unwrap();

        let rows = store.list_swaps_by_user(u1, 10, None).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].tx_hash, "tx_new");
        assert_eq!(rows[1].tx_hash, "tx_old");

        let limited = store.list_swaps_by_user(u1, 1, None).unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].tx_hash, "tx_new");

        let page2 = store
            .list_swaps_by_user(u1, 10, Some((limited[0].created_at, limited[0].tx_hash.as_str())))
            .unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].tx_hash, "tx_old");

        let empty = store
            .list_swaps_by_user("GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCK3LI", 10, None)
            .unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn list_recent_round_trips_filters_and_orders() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = IndexStore::open(&path).unwrap();
        let user = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

        let swap = sample("tx_swap", user, 50, 10);
        store.insert_invocation(&swap).unwrap();

        let mut old = sample("tx_rt_old", user, 100, 1_000_0000);
        old.parsed.function_name = "round_trip_swap".into();
        old.parsed.bridge_token = Some("BRIDGE".into());
        old.parsed.amount_out = Some(1_000_5000);
        store.insert_invocation(&old).unwrap();

        let mut newer = sample("tx_rt_new", user, 200, 2_000_0000);
        newer.parsed.function_name = "round_trip_swap".into();
        newer.parsed.bridge_token = Some("BRIDGE".into());
        newer.parsed.amount_out = Some(2_001_0000);
        store.insert_invocation(&newer).unwrap();

        let mut failed = sample("tx_rt_fail", user, 300, 3_000_0000);
        failed.parsed.function_name = "round_trip_swap".into();
        failed.status = "FAILED".into();
        store.insert_invocation(&failed).unwrap();

        let rows = store.list_recent_round_trips(10, None).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].tx_hash, "tx_rt_new");
        assert_eq!(rows[0].bridge_token.as_deref(), Some("BRIDGE"));
        assert_eq!(rows[1].tx_hash, "tx_rt_old");

        let page2 = store
            .list_recent_round_trips(10, Some((rows[0].created_at, rows[0].tx_hash.as_str())))
            .unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].tx_hash, "tx_rt_old");
    }

    #[test]
    fn replace_invocation_legs_fills_missing_amount_out() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = IndexStore::open(&path).unwrap();
        let user = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

        let mut stored = sample("tx_missing_out", user, 100, 1_000);
        stored.parsed.function_name = "round_trip_swap".into();
        stored.parsed.bridge_token = Some("BRIDGE".into());
        stored.parsed.amount_out = None;
        store.insert_invocation(&stored).unwrap();

        let mut repaired = stored.parsed.clone();
        repaired.amount_out = Some(1_025);
        assert!(store.replace_invocation_legs("tx_missing_out", &repaired).unwrap());

        let rows = store.list_recent_round_trips(10, None).unwrap();
        assert_eq!(rows[0].amount_out.as_deref(), Some("1025"));
    }

    #[test]
    fn open_migrates_legacy_schema_missing_columns() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE swap_invocations (
                    tx_hash TEXT PRIMARY KEY,
                    ledger INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    function_name TEXT NOT NULL,
                    user_address TEXT NOT NULL,
                    token_in TEXT,
                    token_out TEXT,
                    amount_in TEXT NOT NULL,
                    amount_out TEXT
                );
                CREATE TABLE swap_legs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    tx_hash TEXT NOT NULL,
                    leg_index INTEGER NOT NULL,
                    dex_source TEXT NOT NULL,
                    pool_address TEXT NOT NULL,
                    amount_in TEXT,
                    amount_out TEXT
                );
                INSERT INTO swap_invocations (
                    tx_hash, ledger, created_at, status, function_name, user_address,
                    token_in, token_out, amount_in, amount_out
                ) VALUES (
                    'tx_rt', 1, 100, 'SUCCESS', 'round_trip_swap', 'USER',
                    'BASE', 'BASE', '1000', '1100'
                );
                ",
            )
            .unwrap();
        }

        let store = IndexStore::open(&path).unwrap();
        assert!(store.table_has_column("swap_invocations", "bridge_token").unwrap());
        assert!(store.table_has_column("swap_invocations", "is_split").unwrap());
        assert!(store.table_has_column("swap_legs", "amount_is_actual").unwrap());

        let rows = store.list_recent_round_trips(10, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tx_hash, "tx_rt");
        assert!(rows[0].bridge_token.is_none());
    }

    #[test]
    fn envelope_enrichment_preserves_event_leg_amounts() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        let mut event_record = sample("tx", "USER", 100, 100);
        event_record.parsed.legs = vec![
            ParsedLeg {
                leg_index: 0,
                dex_source: "soroswap".into(),
                pool_address: "POOL_1".into(),
                token_in: None,
                token_out: None,
                amount_in: Some(100),
                amount_out: Some(55),
                amount_is_actual: true,
            },
            ParsedLeg {
                leg_index: 1,
                dex_source: "phoenix".into(),
                pool_address: "POOL_2".into(),
                token_in: None,
                token_out: None,
                amount_in: Some(55),
                amount_out: Some(40),
                amount_is_actual: true,
            },
        ];
        store.insert_invocation(&event_record).unwrap();

        let mut envelope = event_record.parsed.clone();
        envelope.legs[0].token_in = Some("TOKEN_A".into());
        envelope.legs[0].token_out = Some("TOKEN_B".into());
        envelope.legs[1].token_in = Some("TOKEN_B".into());
        envelope.legs[1].token_out = Some("TOKEN_C".into());
        envelope.legs[1].amount_in = Some(100);
        store.replace_invocation_legs("tx", &envelope).unwrap();

        let mut stmt = store
            .conn()
            .prepare("SELECT token_in, amount_in FROM swap_legs WHERE tx_hash = 'tx' ORDER BY id")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![("TOKEN_A".into(), "100".into()), ("TOKEN_B".into(), "55".into())]
        );
    }

    const OWNER: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    const OTHER: &str = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    fn seed_open_order(store: &IndexStore, order_id: i64, owner: &str, amount: &str, at: i64) {
        store
            .upsert_created(
                order_id,
                owner,
                "TOKEN_IN",
                "TOKEN_OUT",
                amount,
                amount,
                "10000000",
                500,
                100,
                100,
                at,
                at,
            )
            .unwrap();
    }

    #[test]
    fn upsert_created_inserts_open_order() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        store
            .upsert_created(
                1, OWNER, "USDC", "XLM", "1000", "1000", "2500000", 999, 10, 10, 1_000, 1_000,
            )
            .unwrap();

        let rows = store.list_by_owner(OWNER, Some("all")).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.order_id, 1);
        assert_eq!(row.owner, OWNER);
        assert_eq!(row.token_in, "USDC");
        assert_eq!(row.token_out, "XLM");
        assert_eq!(row.amount_in_initial.as_deref(), Some("1000"));
        assert_eq!(row.amount_in_remaining, "1000");
        assert_eq!(row.limit_out_per_in_e7, "2500000");
        assert_eq!(row.expires_ledger, 999);
        assert_eq!(row.status, "open");
        assert_eq!(row.created_ledger, Some(10));
        assert_eq!(row.updated_ledger, 10);
        assert_eq!(row.created_at, Some(1_000));
        assert_eq!(row.updated_at, 1_000);
    }

    #[test]
    fn apply_filled_updates_remaining_and_status() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        seed_open_order(&store, 1, OWNER, "1000", 1_000);

        store.apply_filled(1, "400", 101, 1_100).unwrap();

        let row = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(row.amount_in_remaining, "400");
        assert_eq!(row.status, "open");
        assert_eq!(row.updated_ledger, 101);
        assert_eq!(row.updated_at, 1_100);

        store.apply_filled(1, "0", 102, 1_200).unwrap();
        let row = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(row.amount_in_remaining, "0");
        assert_eq!(row.status, "filled");
    }

    #[test]
    fn apply_closed_sets_status_and_zeroes_remaining() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        seed_open_order(&store, 1, OWNER, "500", 1_000);

        store.apply_closed(1, "cancelled", 110, 2_000).unwrap();
        let row = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.amount_in_remaining, "0");
        assert_eq!(row.updated_ledger, 110);

        seed_open_order(&store, 2, OWNER, "500", 1_001);
        store.apply_closed(2, "expired", 111, 2_001).unwrap();
        let row = store
            .list_by_owner(OWNER, Some("all"))
            .unwrap()
            .into_iter()
            .find(|r| r.order_id == 2)
            .unwrap();
        assert_eq!(row.status, "expired");
        assert_eq!(row.amount_in_remaining, "0");
    }

    #[test]
    fn apply_closed_rejects_invalid_status() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        seed_open_order(&store, 1, OWNER, "500", 1_000);

        let err = store.apply_closed(1, "filled", 110, 2_000).unwrap_err();
        assert!(err.to_string().contains("invalid closed status"));
    }

    #[test]
    fn list_by_owner_filters_owner_and_status() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();
        seed_open_order(&store, 1, OWNER, "100", 1_000);
        seed_open_order(&store, 2, OWNER, "200", 900);
        seed_open_order(&store, 3, OTHER, "300", 800);
        store.apply_filled(2, "0", 120, 1_500).unwrap();

        let open = store.list_by_owner(OWNER, None).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].order_id, 1);

        let open_explicit = store.list_by_owner(OWNER, Some("open")).unwrap();
        assert_eq!(open_explicit.len(), 1);

        let all = store.list_by_owner(OWNER, Some("all")).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].order_id, 2);
        assert_eq!(all[1].order_id, 1);

        let other_open = store.list_by_owner(OTHER, None).unwrap();
        assert_eq!(other_open.len(), 1);
        assert_eq!(other_open[0].order_id, 3);
    }

    #[test]
    fn limit_order_lifecycle_sequence() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        store
            .upsert_created(
                42, OWNER, "USDC", "XLM", "1000", "1000", "2500000", 999, 50, 50, 100, 100,
            )
            .unwrap();
        assert_eq!(store.list_by_owner(OWNER, None).unwrap().len(), 1);

        store.apply_filled(42, "600", 60, 200).unwrap();
        let partial = store.list_by_owner(OWNER, None).unwrap().pop().unwrap();
        assert_eq!(partial.status, "open");
        assert_eq!(partial.amount_in_remaining, "600");

        store.apply_filled(42, "0", 70, 300).unwrap();
        assert!(store.list_by_owner(OWNER, None).unwrap().is_empty());
        let filled = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(filled.status, "filled");

        store
            .upsert_created(43, OWNER, "USDC", "XLM", "500", "500", "2500000", 999, 80, 80, 400, 400)
            .unwrap();
        store.apply_closed(43, "cancelled", 90, 500).unwrap();
        assert!(store.list_by_owner(OWNER, None).unwrap().is_empty());
        let cancelled = store
            .list_by_owner(OWNER, Some("all"))
            .unwrap()
            .into_iter()
            .find(|r| r.order_id == 43)
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.amount_in_remaining, "0");
    }

    #[test]
    fn replay_upsert_created_preserves_terminal_status() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        seed_open_order(&store, 1, OWNER, "1000", 1_000);
        store.apply_filled(1, "0", 110, 2_000).unwrap();

        store
            .upsert_created(
                1, OWNER, "USDC", "XLM", "9999", "9999", "1111111", 1, 999, 999, 9_999, 9_999,
            )
            .unwrap();

        let row = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(row.status, "filled");
        assert_eq!(row.amount_in_remaining, "0");
        assert_eq!(row.amount_in_initial.as_deref(), Some("1000"));
        assert_eq!(row.updated_ledger, 110);

        seed_open_order(&store, 2, OWNER, "500", 1_100);
        store.apply_closed(2, "cancelled", 120, 2_100).unwrap();

        store
            .upsert_created(
                2, OWNER, "USDC", "XLM", "8888", "8888", "2222222", 2, 888, 888, 8_888, 8_888,
            )
            .unwrap();

        let row = store
            .list_by_owner(OWNER, Some("all"))
            .unwrap()
            .into_iter()
            .find(|r| r.order_id == 2)
            .unwrap();
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.amount_in_remaining, "0");
        assert_eq!(row.amount_in_initial.as_deref(), Some("500"));
        assert_eq!(row.updated_ledger, 120);
    }

    #[test]
    fn apply_filled_returns_false_when_order_missing() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        assert!(!store.apply_filled(99, "100", 110, 2_000).unwrap());
    }

    #[test]
    fn apply_closed_returns_false_when_order_missing() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        assert!(!store.apply_closed(99, "cancelled", 110, 2_000).unwrap());
    }

    #[test]
    fn late_apply_filled_after_cancel_is_noop() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        seed_open_order(&store, 1, OWNER, "1000", 1_000);
        store.apply_closed(1, "cancelled", 110, 2_000).unwrap();

        assert!(!store.apply_filled(1, "400", 120, 2_100).unwrap());

        let row = store.list_by_owner(OWNER, Some("all")).unwrap().pop().unwrap();
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.amount_in_remaining, "0");
        assert_eq!(row.updated_ledger, 110);
        assert_eq!(row.updated_at, 2_000);
    }

    #[test]
    fn order_ids_are_scoped_to_the_escrow_contract() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("test.db")).unwrap();

        for (contract, amount) in [("ESCROW_A", "100"), ("ESCROW_B", "200")] {
            store
                .upsert_created_for(
                    contract, 1, OWNER, "USDC", "XLM", amount, amount, "2500000", 999, 10, 10, 1_000, 1_000,
                )
                .unwrap();
        }

        let a = store.list_by_owner_for("ESCROW_A", OWNER, Some("all")).unwrap();
        let b = store.list_by_owner_for("ESCROW_B", OWNER, Some("all")).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].order_id, 1);
        assert_eq!(b[0].order_id, 1);
        assert_eq!(a[0].amount_in_initial.as_deref(), Some("100"));
        assert_eq!(b[0].amount_in_initial.as_deref(), Some("200"));

        assert!(store.apply_filled_for("ESCROW_A", 1, "0", 20, 2_000).unwrap());
        assert_eq!(
            store.list_by_owner_for("ESCROW_A", OWNER, Some("all")).unwrap()[0].status,
            "filled"
        );
        assert_eq!(
            store.list_by_owner_for("ESCROW_B", OWNER, Some("all")).unwrap()[0].status,
            "open"
        );
    }
}

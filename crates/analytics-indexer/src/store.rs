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

#[derive(Debug, Clone)]
pub struct LimitOrderRow {
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

pub struct IndexStore {
    conn: Connection,
}

fn map_limit_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LimitOrderRow> {
    Ok(LimitOrderRow {
        order_id: row.get(0)?,
        owner: row.get(1)?,
        token_in: row.get(2)?,
        token_out: row.get(3)?,
        amount_in_initial: row.get(4)?,
        amount_in_remaining: row.get(5)?,
        limit_out_per_in_e7: row.get(6)?,
        expires_ledger: row.get::<_, i64>(7)? as u32,
        status: row.get(8)?,
        created_ledger: row.get::<_, Option<i64>>(9)?.map(|v| v as u32),
        updated_ledger: row.get::<_, i64>(10)? as u32,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
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
            CREATE INDEX IF NOT EXISTS idx_swap_invocations_user_created
              ON swap_invocations(user_address, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_swap_legs_dex ON swap_legs(dex_source);

            CREATE TABLE IF NOT EXISTS limit_orders (
              order_id INTEGER PRIMARY KEY,
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
              updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_limit_orders_owner ON limit_orders(owner, status);
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

    /// Replace hop rows for an existing invocation (e.g. fix serial `leg_index`
    /// after re-parsing the envelope). Also updates `is_split`.
    pub fn replace_invocation_legs(&self, tx_hash: &str, parsed: &crate::parser::ParsedInvocation) -> Result<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM swap_invocations WHERE tx_hash = ?1",
            params![tx_hash],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
        self.conn
            .execute("DELETE FROM swap_legs WHERE tx_hash = ?1", params![tx_hash])?;
        self.conn.execute(
            "UPDATE swap_invocations SET is_split = ?1 WHERE tx_hash = ?2",
            params![parsed.is_split as i32, tx_hash],
        )?;
        for leg in &parsed.legs {
            self.insert_leg(tx_hash, leg)?;
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
        self.conn.execute(
            "INSERT OR IGNORE INTO limit_orders (
                order_id, owner, token_in, token_out, amount_in_initial, amount_in_remaining,
                limit_out_per_in_e7, expires_ledger, status, created_ledger, updated_ledger,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'open', ?9, ?10, ?11, ?12)",
            params![
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
        let status = if amount_in_remaining == "0" { "filled" } else { "open" };
        let updated = self.conn.execute(
            "UPDATE limit_orders
             SET amount_in_remaining = ?1, status = ?2, updated_ledger = ?3, updated_at = ?4
             WHERE order_id = ?5 AND status = 'open'",
            params![amount_in_remaining, status, updated_ledger, updated_at, order_id,],
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
        let updated = self.conn.execute(
            "UPDATE limit_orders
             SET status = ?1, amount_in_remaining = '0', updated_ledger = ?2, updated_at = ?3
             WHERE order_id = ?4 AND status = 'open'",
            params![status, updated_ledger, updated_at, order_id],
        )?;
        Ok(updated > 0)
    }

    pub fn list_by_owner(&self, owner: &str, status_filter: Option<&str>) -> Result<Vec<LimitOrderRow>> {
        let sql = match status_filter {
            Some("all") => {
                "SELECT order_id, owner, token_in, token_out, amount_in_initial,
                        amount_in_remaining, limit_out_per_in_e7, expires_ledger, status,
                        created_ledger, updated_ledger, created_at, updated_at
                 FROM limit_orders
                 WHERE owner = ?1
                 ORDER BY updated_at DESC, order_id DESC"
            }
            _ => {
                "SELECT order_id, owner, token_in, token_out, amount_in_initial,
                        amount_in_remaining, limit_out_per_in_e7, expires_ledger, status,
                        created_ledger, updated_ledger, created_at, updated_at
                 FROM limit_orders
                 WHERE owner = ?1 AND status = 'open'
                 ORDER BY updated_at DESC, order_id DESC"
            }
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![owner], map_limit_order_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
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
            parsed: ParsedInvocation {
                function_name: "swap".into(),
                user_address: user.into(),
                token_in: Some("TOKEN_IN".into()),
                token_out: Some("TOKEN_OUT".into()),
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
}

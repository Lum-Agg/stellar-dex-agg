//! SQLite persistence for quote-sampled USDC price ticks.

use {
    anyhow::{Context, Result},
    rusqlite::{params, Connection},
    std::{path::Path, sync::Mutex},
};

#[derive(Debug, Clone, PartialEq)]
pub struct PriceTick {
    pub token: String,
    pub ts: i64,
    pub price_usdc: f64,
    pub via: String,
}

pub struct PriceStore {
    conn: Mutex<Connection>,
}

impl PriceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("open price sqlite db")?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS price_ticks (
              token TEXT NOT NULL,
              ts INTEGER NOT NULL,
              price_usdc REAL NOT NULL,
              via TEXT NOT NULL,
              PRIMARY KEY (token, ts)
            );
            CREATE INDEX IF NOT EXISTS idx_price_ticks_token_ts ON price_ticks(token, ts DESC);
            ",
        )?;
        Ok(())
    }

    pub fn insert_tick(&self, token: &str, ts: i64, price_usdc: f64, via: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO price_ticks (token, ts, price_usdc, via) VALUES (?1, ?2, ?3, ?4)",
            params![token, ts, price_usdc, via],
        )?;
        Ok(())
    }

    pub fn latest(&self, token: &str) -> Result<Option<PriceTick>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT token, ts, price_usdc, via
             FROM price_ticks
             WHERE token = ?1
             ORDER BY ts DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![token])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_tick(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn latest_many(&self, tokens: &[String]) -> Result<Vec<PriceTick>> {
        let mut out = Vec::with_capacity(tokens.len());
        for token in tokens {
            if let Some(tick) = self.latest(token)? {
                out.push(tick);
            }
        }
        Ok(out)
    }

    pub fn history(&self, token: &str, from_ts: i64, to_ts: i64) -> Result<Vec<PriceTick>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT token, ts, price_usdc, via
             FROM price_ticks
             WHERE token = ?1 AND ts >= ?2 AND ts <= ?3
             ORDER BY ts ASC",
        )?;
        let rows = stmt.query_map(params![token, from_ts, to_ts], row_to_tick)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn prune_older_than(&self, cutoff_ts: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM price_ticks WHERE ts < ?1", params![cutoff_ts])?;
        Ok(n)
    }
}

fn row_to_tick(row: &rusqlite::Row<'_>) -> rusqlite::Result<PriceTick> {
    Ok(PriceTick {
        token: row.get(0)?,
        ts: row.get(1)?,
        price_usdc: row.get(2)?,
        via: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use {super::*, tempfile::tempdir};

    #[test]
    fn insert_and_latest() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 100, 1.5, "usdc").unwrap();
        store.insert_tick("TOK", 200, 1.6, "usdc").unwrap();
        let latest = store.latest("TOK").unwrap().unwrap();
        assert_eq!(latest.ts, 200);
        assert!((latest.price_usdc - 1.6).abs() < 1e-9);
    }

    #[test]
    fn history_range_filter() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 1000, 1.0, "usdc").unwrap();
        store.insert_tick("TOK", 2000, 2.0, "usdc").unwrap();
        store.insert_tick("TOK", 3000, 3.0, "usdc").unwrap();
        let pts = store.history("TOK", 1500, 3000).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].ts, 2000);
        assert_eq!(pts[1].ts, 3000);
    }

    #[test]
    fn prune_older_than() {
        let dir = tempdir().unwrap();
        let store = PriceStore::open(dir.path().join("p.db")).unwrap();
        store.insert_tick("TOK", 100, 1.0, "usdc").unwrap();
        store.insert_tick("TOK", 200, 2.0, "usdc").unwrap();
        let n = store.prune_older_than(150).unwrap();
        assert_eq!(n, 1);
        assert!(store.latest("TOK").unwrap().unwrap().ts == 200);
    }
}

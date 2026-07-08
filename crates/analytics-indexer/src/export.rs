//! Daily aggregate export for analytics dashboard / API handoff.

use {
    crate::store::IndexStore,
    anyhow::Result,
    chrono::{DateTime, TimeZone, Utc},
    rusqlite::params,
    serde::Serialize,
    std::collections::BTreeMap,
};
#[derive(Debug, Serialize)]
pub struct DailyStats {
    pub day: String,
    pub tx_count: u64,
    pub unique_users: u64,
    pub total_amount_in: i128,
    pub by_function: BTreeMap<String, u64>,
    pub by_dex: BTreeMap<String, u64>,
    pub split_swap_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
}

pub fn export_daily(store: &IndexStore, day: &str) -> Result<DailyStats> {
    let start = parse_day_start(day)?;
    let end = start + chrono::Duration::days(1);
    export_range(store, start.timestamp(), end.timestamp(), day)
}

pub fn export_all_days(store: &IndexStore) -> Result<Vec<DailyStats>> {
    let mut stmt = store
        .conn()
        .prepare("SELECT DISTINCT date(created_at, 'unixepoch') AS d FROM swap_invocations ORDER BY d")?;
    let days: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;

    days.iter().map(|d| export_daily(store, d)).collect()
}

fn export_range(store: &IndexStore, start_ts: i64, end_ts: i64, day_label: &str) -> Result<DailyStats> {
    let tx_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM swap_invocations WHERE created_at >= ?1 AND created_at < ?2",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let unique_users: i64 = store.conn().query_row(
        "SELECT COUNT(DISTINCT user_address) FROM swap_invocations WHERE created_at >= ?1 AND created_at < ?2",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let total_amount_in: i64 = store.conn().query_row(
        "SELECT COALESCE(SUM(CAST(amount_in AS INTEGER)), 0) FROM swap_invocations WHERE created_at >= ?1 AND created_at < ?2",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let split_swap_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM swap_invocations WHERE created_at >= ?1 AND created_at < ?2 AND is_split = 1",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let success_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM swap_invocations WHERE created_at >= ?1 AND created_at < ?2 AND status = 'SUCCESS'",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let failed_count = tx_count - success_count;

    let mut by_function = BTreeMap::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT function_name, COUNT(*) FROM swap_invocations
             WHERE created_at >= ?1 AND created_at < ?2 GROUP BY function_name",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, count) = row?;
            by_function.insert(name, count as u64);
        }
    }

    let mut by_dex = BTreeMap::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT l.dex_source, COUNT(*) FROM swap_legs l
             JOIN swap_invocations i ON i.tx_hash = l.tx_hash
             WHERE i.created_at >= ?1 AND i.created_at < ?2
             GROUP BY l.dex_source",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (dex, count) = row?;
            by_dex.insert(dex, count as u64);
        }
    }

    Ok(DailyStats {
        day: day_label.to_string(),
        tx_count: tx_count as u64,
        unique_users: unique_users as u64,
        total_amount_in: total_amount_in as i128,
        by_function,
        by_dex,
        split_swap_count: split_swap_count as u64,
        success_count: success_count as u64,
        failed_count: failed_count as u64,
    })
}

fn parse_day_start(day: &str) -> Result<DateTime<Utc>> {
    Utc.with_ymd_and_hms(day[0..4].parse()?, day[5..7].parse()?, day[8..10].parse()?, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid day {}", day))
}

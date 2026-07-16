//! Daily aggregate export for analytics dashboard / API handoff.

use {
    crate::store::IndexStore,
    anyhow::Result,
    chrono::{DateTime, TimeZone, Utc},
    rusqlite::params,
    serde::Serialize,
    std::collections::BTreeMap,
};

#[derive(Debug, Serialize, Clone)]
pub struct TokenVolume {
    pub token: String,
    /// Sum of entry `amount_in` (native token smallest units).
    pub amount_in: i128,
    /// Sum of `amount_in × serial_hops` (native units). Serial hops =
    /// `max(leg_index)+1` so parallel split legs sharing an index count once.
    pub routed_volume: i128,
}

#[derive(Debug, Serialize)]
pub struct DailyStats {
    pub day: String,
    pub tx_count: u64,
    pub unique_users: u64,
    /// Sum of entry `amount_in` across invocations (mixed tokens — do not treat
    /// as XLM). Prefer `by_token` / USD fields for reporting.
    pub total_amount_in: i128,
    /// Sum of `amount_in × serial_hops` across invocations (mixed token units).
    pub total_routed_dex_volume: i128,
    /// Per-input-token breakdown of entry + hop-weighted routed volume.
    pub by_token: Vec<TokenVolume>,
    pub by_function: BTreeMap<String, u64>,
    /// Leg counts per DEX venue.
    pub by_dex: BTreeMap<String, u64>,
    pub split_swap_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    /// XLM/USD for this UTC day (when enrichment ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xlm_usd: Option<f64>,
    /// Entry notional in USD (per-token priced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount_in_usd: Option<f64>,
    /// Hop-weighted routed DEX volume in USD (primary contribution metric).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_routed_dex_volume_usd: Option<f64>,
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

    // Entry notional × serial hop count. Hops = max(leg_index)+1 (parallel
    // splits share an index and are not multiplied).
    // Example: a→b→c with 100 in → 2 hops → 200 routed units of token_in.
    let total_routed_dex_volume: i64 = store.conn().query_row(
        "SELECT COALESCE(SUM(
            CAST(i.amount_in AS INTEGER) * (
              SELECT COALESCE(MAX(l.leg_index), 0) + 1
              FROM swap_legs l
              WHERE l.tx_hash = i.tx_hash
            )
         ), 0)
         FROM swap_invocations i
         WHERE i.created_at >= ?1 AND i.created_at < ?2",
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

    let mut by_token = Vec::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT COALESCE(token_in, '') AS tok,
                    COALESCE(SUM(CAST(i.amount_in AS INTEGER)), 0),
                    COALESCE(SUM(
                      CAST(i.amount_in AS INTEGER) * (
                        SELECT COALESCE(MAX(l.leg_index), 0) + 1
                        FROM swap_legs l WHERE l.tx_hash = i.tx_hash
                      )
                    ), 0)
             FROM swap_invocations i
             WHERE i.created_at >= ?1 AND i.created_at < ?2
             GROUP BY tok
             ORDER BY 3 DESC",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (token, amount_in, routed) = row?;
            if token.is_empty() && amount_in == 0 {
                continue;
            }
            by_token.push(TokenVolume {
                token,
                amount_in: amount_in as i128,
                routed_volume: routed as i128,
            });
        }
    }

    Ok(DailyStats {
        day: day_label.to_string(),
        tx_count: tx_count as u64,
        unique_users: unique_users as u64,
        total_amount_in: total_amount_in as i128,
        total_routed_dex_volume: total_routed_dex_volume as i128,
        by_token,
        by_function,
        by_dex,
        split_swap_count: split_swap_count as u64,
        success_count: success_count as u64,
        failed_count: failed_count as u64,
        xlm_usd: None,
        total_amount_in_usd: None,
        total_routed_dex_volume_usd: None,
    })
}

fn parse_day_start(day: &str) -> Result<DateTime<Utc>> {
    Utc.with_ymd_and_hms(day[0..4].parse()?, day[5..7].parse()?, day[8..10].parse()?, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid day {}", day))
}

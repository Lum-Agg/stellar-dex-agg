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
    /// Actual routed amount for this entry token, retained for API
    /// compatibility.
    pub routed_volume: i128,
    /// Executed legs whose input matches this entry token.
    pub routed_leg_count: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct RoutedTokenVolume {
    pub token: String,
    /// Sum of actual input processed by DEX legs denominated in this token.
    pub routed_volume: i128,
    pub routed_leg_count: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct RoundTripSurplus {
    pub base_token: String,
    pub tx_count: u64,
    /// Actual base token supplied to successful round-trip calls.
    pub amount_in: i128,
    /// Actual on-chain `amount_out - amount_in`, before transaction fees.
    pub gross_surplus: i128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gross_surplus_usd: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RoundTripBridgeStats {
    pub bridge_token: String,
    pub tx_count: u64,
    pub amount_in: i128,
    pub gross_surplus: i128,
}

#[derive(Debug, Serialize)]
pub struct DailyStats {
    pub day: String,
    pub tx_count: u64,
    pub unique_users: u64,
    /// Sum of entry `amount_in` across invocations (mixed tokens — do not treat
    /// as XLM). Prefer `by_token` / USD fields for reporting.
    pub total_amount_in: i128,
    /// Sum of actual DEX leg inputs (mixed token units; use `by_token` or USD).
    pub total_routed_dex_volume: i128,
    /// Successful legs with event-derived actual input amounts.
    pub routed_leg_count: u64,
    /// Legs included in the USD routed-volume subtotal.
    pub routed_priced_leg_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routed_pricing_coverage: Option<f64>,
    /// Entry notional grouped by invocation `token_in`. Kept stable for
    /// external consumers including DefiLlama.
    pub by_token: Vec<TokenVolume>,
    /// Actual DEX leg inputs grouped by each leg's `token_in`.
    pub routed_by_token: Vec<RoutedTokenVolume>,
    pub by_function: BTreeMap<String, u64>,
    /// Leg counts per DEX venue.
    pub by_dex: BTreeMap<String, u64>,
    /// Successful round trips with a complete on-chain return amount.
    pub round_trip_count: u64,
    /// Gross round-trip surplus by base token. This is not net P&L because
    /// transaction fees are not available from aggregator events.
    pub round_trip_by_token: Vec<RoundTripSurplus>,
    /// Successful round trips grouped by intermediary bridge token.
    pub round_trip_by_bridge: Vec<RoundTripBridgeStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_trip_gross_surplus_usd: Option<f64>,
    pub split_swap_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    /// XLM/USD for this UTC day (when enrichment ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xlm_usd: Option<f64>,
    /// Entry notional in USD (per-token priced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount_in_usd: Option<f64>,
    /// Sum of actual DEX leg inputs in USD for tokens with known prices.
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
        "SELECT COUNT(*) FROM swap_invocations
         WHERE created_at >= ?1 AND created_at < ?2 AND status = 'SUCCESS'",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let unique_users: i64 = store.conn().query_row(
        "SELECT COUNT(DISTINCT user_address) FROM swap_invocations
         WHERE created_at >= ?1 AND created_at < ?2 AND status = 'SUCCESS'",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let total_amount_in: i64 = store.conn().query_row(
        "SELECT COALESCE(SUM(CAST(amount_in AS INTEGER)), 0)
         FROM swap_invocations
         WHERE created_at >= ?1 AND created_at < ?2 AND status = 'SUCCESS'",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let total_routed_dex_volume: i64 = store.conn().query_row(
        "SELECT COALESCE(SUM(CAST(l.amount_in AS INTEGER)), 0)
         FROM swap_legs l
         JOIN swap_invocations i ON i.tx_hash = l.tx_hash
         WHERE i.created_at >= ?1 AND i.created_at < ?2
           AND i.status = 'SUCCESS'
           AND l.amount_in IS NOT NULL
           AND l.amount_is_actual = 1",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;
    let routed_leg_count: i64 = store.conn().query_row(
        "SELECT COUNT(*)
         FROM swap_legs l
         JOIN swap_invocations i ON i.tx_hash = l.tx_hash
         WHERE i.created_at >= ?1 AND i.created_at < ?2
           AND i.status = 'SUCCESS'
           AND l.amount_in IS NOT NULL
           AND l.amount_is_actual = 1",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let split_swap_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM swap_invocations
         WHERE created_at >= ?1 AND created_at < ?2
           AND status = 'SUCCESS' AND is_split = 1",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;

    let failed_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM swap_invocations
         WHERE created_at >= ?1 AND created_at < ?2 AND status != 'SUCCESS'",
        params![start_ts, end_ts],
        |r| r.get(0),
    )?;
    let success_count = tx_count;

    let mut by_function = BTreeMap::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT function_name, COUNT(*) FROM swap_invocations
             WHERE created_at >= ?1 AND created_at < ?2
               AND status = 'SUCCESS'
             GROUP BY function_name",
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
               AND i.status = 'SUCCESS'
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

    let mut entry_totals: BTreeMap<String, i128> = BTreeMap::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT token_in, COALESCE(SUM(CAST(amount_in AS INTEGER)), 0)
             FROM swap_invocations
             WHERE created_at >= ?1 AND created_at < ?2
               AND status = 'SUCCESS'
               AND token_in IS NOT NULL
             GROUP BY token_in
             ORDER BY 2 DESC",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (token, amount_in) = row?;
            entry_totals.insert(token, amount_in as i128);
        }
    }
    let mut routed_by_token = Vec::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT l.token_in,
                    COALESCE(SUM(CAST(l.amount_in AS INTEGER)), 0),
                    COUNT(*)
             FROM swap_legs l
             JOIN swap_invocations i ON i.tx_hash = l.tx_hash
             WHERE i.created_at >= ?1 AND i.created_at < ?2
               AND i.status = 'SUCCESS'
               AND l.token_in IS NOT NULL
               AND l.amount_in IS NOT NULL
               AND l.amount_is_actual = 1
             GROUP BY l.token_in
             ORDER BY 2 DESC",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (token, routed, leg_count) = row?;
            routed_by_token.push(RoutedTokenVolume {
                token,
                routed_volume: routed as i128,
                routed_leg_count: leg_count as u64,
            });
        }
    }
    let mut by_token: Vec<TokenVolume> = entry_totals
        .into_iter()
        .map(|(token, amount_in)| {
            let routed = routed_by_token.iter().find(|row| row.token == token);
            TokenVolume {
                token,
                amount_in,
                routed_volume: routed.map(|row| row.routed_volume).unwrap_or(0),
                routed_leg_count: routed.map(|row| row.routed_leg_count).unwrap_or(0),
            }
        })
        .collect();
    by_token.sort_by(|a, b| b.amount_in.cmp(&a.amount_in));

    let mut round_trip_by_token = Vec::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT COALESCE(token_in, '') AS base,
                    COUNT(*),
                    COALESCE(SUM(CAST(amount_in AS INTEGER)), 0),
                    COALESCE(SUM(
                      CAST(amount_out AS INTEGER) - CAST(amount_in AS INTEGER)
                    ), 0)
             FROM swap_invocations
             WHERE created_at >= ?1 AND created_at < ?2
               AND function_name = 'round_trip_swap'
               AND status = 'SUCCESS'
               AND amount_out IS NOT NULL
             GROUP BY base
             ORDER BY 4 DESC",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (base_token, count, amount_in, gross_surplus) = row?;
            round_trip_by_token.push(RoundTripSurplus {
                base_token,
                tx_count: count as u64,
                amount_in: amount_in as i128,
                gross_surplus: gross_surplus as i128,
                gross_surplus_usd: None,
            });
        }
    }
    let round_trip_count = round_trip_by_token.iter().map(|row| row.tx_count).sum();

    let mut round_trip_by_bridge = Vec::new();
    {
        let mut stmt = store.conn().prepare(
            "SELECT COALESCE(bridge_token, '') AS bridge,
                    COUNT(*),
                    COALESCE(SUM(CAST(amount_in AS INTEGER)), 0),
                    COALESCE(SUM(
                      CAST(amount_out AS INTEGER) - CAST(amount_in AS INTEGER)
                    ), 0)
             FROM swap_invocations
             WHERE created_at >= ?1 AND created_at < ?2
               AND function_name = 'round_trip_swap'
               AND status = 'SUCCESS'
               AND amount_out IS NOT NULL
             GROUP BY bridge
             ORDER BY 4 DESC",
        )?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok(RoundTripBridgeStats {
                bridge_token: row.get(0)?,
                tx_count: row.get::<_, i64>(1)? as u64,
                amount_in: row.get::<_, i64>(2)? as i128,
                gross_surplus: row.get::<_, i64>(3)? as i128,
            })
        })?;
        for row in rows {
            round_trip_by_bridge.push(row?);
        }
    }

    Ok(DailyStats {
        day: day_label.to_string(),
        tx_count: tx_count as u64,
        unique_users: unique_users as u64,
        total_amount_in: total_amount_in as i128,
        total_routed_dex_volume: total_routed_dex_volume as i128,
        routed_leg_count: routed_leg_count as u64,
        routed_priced_leg_count: 0,
        routed_pricing_coverage: None,
        by_token,
        routed_by_token,
        by_function,
        by_dex,
        round_trip_count,
        round_trip_by_token,
        round_trip_by_bridge,
        round_trip_gross_surplus_usd: None,
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            parser::{ParsedInvocation, ParsedLeg},
            store::{IndexStore, StoredInvocation},
        },
        tempfile::tempdir,
    };

    fn insert_round_trip(store: &IndexStore, tx_hash: &str, status: &str, amount_in: i128, amount_out: i128) {
        store
            .insert_invocation(&StoredInvocation {
                tx_hash: tx_hash.into(),
                ledger: 1,
                created_at: 1_784_851_200,
                status: status.into(),
                failure_reason: None,
                parsed: ParsedInvocation {
                    function_name: "round_trip_swap".into(),
                    user_address: "USER".into(),
                    token_in: Some("BASE".into()),
                    token_out: Some("BASE".into()),
                    bridge_token: Some("BRIDGE".into()),
                    amount_in,
                    amount_out: Some(amount_out),
                    is_split: false,
                    legs: Vec::new(),
                },
            })
            .unwrap();
    }

    #[test]
    fn round_trip_surplus_uses_successful_on_chain_amounts() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("stats.db")).unwrap();
        insert_round_trip(&store, "success-1", "SUCCESS", 1_000, 1_025);
        insert_round_trip(&store, "success-2", "SUCCESS", 2_000, 2_040);
        insert_round_trip(&store, "failed", "FAILED", 3_000, 9_000);

        let stats = export_daily(&store, "2026-07-24").unwrap();
        assert_eq!(stats.round_trip_count, 2);
        assert_eq!(stats.round_trip_by_token.len(), 1);
        assert_eq!(stats.round_trip_by_token[0].amount_in, 3_000);
        assert_eq!(stats.round_trip_by_token[0].gross_surplus, 65);
        assert_eq!(stats.round_trip_by_bridge.len(), 1);
        assert_eq!(stats.round_trip_by_bridge[0].bridge_token, "BRIDGE");
        assert_eq!(stats.round_trip_by_bridge[0].tx_count, 2);
    }

    #[test]
    fn routed_volume_sums_actual_leg_inputs_by_token() {
        let dir = tempdir().unwrap();
        let store = IndexStore::open(dir.path().join("stats.db")).unwrap();
        store
            .insert_invocation(&StoredInvocation {
                tx_hash: "multi-hop".into(),
                ledger: 1,
                created_at: 1_784_851_200,
                status: "SUCCESS".into(),
                failure_reason: None,
                parsed: ParsedInvocation {
                    function_name: "swap".into(),
                    user_address: "USER".into(),
                    token_in: Some("TOKEN_A".into()),
                    token_out: Some("TOKEN_C".into()),
                    bridge_token: None,
                    amount_in: 100,
                    amount_out: Some(80),
                    is_split: false,
                    legs: vec![
                        ParsedLeg {
                            leg_index: 0,
                            dex_source: "soroswap".into(),
                            pool_address: "POOL_1".into(),
                            token_in: Some("TOKEN_A".into()),
                            token_out: Some("TOKEN_B".into()),
                            amount_in: Some(100),
                            amount_out: Some(47),
                            amount_is_actual: true,
                        },
                        ParsedLeg {
                            leg_index: 1,
                            dex_source: "phoenix".into(),
                            pool_address: "POOL_2".into(),
                            token_in: Some("TOKEN_B".into()),
                            token_out: Some("TOKEN_C".into()),
                            amount_in: Some(47),
                            amount_out: Some(80),
                            amount_is_actual: true,
                        },
                    ],
                },
            })
            .unwrap();

        let stats = export_daily(&store, "2026-07-24").unwrap();
        assert_eq!(stats.total_routed_dex_volume, 147);
        assert_eq!(
            stats.by_token.len(),
            1,
            "intermediate tokens must not alter entry notional"
        );
        let token_a = stats.by_token.iter().find(|row| row.token == "TOKEN_A").unwrap();
        let token_b = stats.routed_by_token.iter().find(|row| row.token == "TOKEN_B").unwrap();
        assert_eq!(
            (token_a.amount_in, token_a.routed_volume, token_a.routed_leg_count),
            (100, 100, 1)
        );
        assert_eq!((token_b.routed_volume, token_b.routed_leg_count), (47, 1));
    }
}

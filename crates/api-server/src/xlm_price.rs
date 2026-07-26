//! Historical XLM/USD for analytics stats (day-level).

use {analytics_indexer::export::DailyStats, serde::Deserialize, std::collections::HashMap, tracing::warn};

/// Well-known mainnet SAC → USD price source for stats enrichment.
const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
/// Stellar classic SACs use 7 decimals.
const TOKEN_DECIMALS: f64 = 1e7;

/// Attach day-level USD using per-token pricing (not “everything is XLM”).
pub async fn enrich_daily_with_historical_usd(daily: &mut [DailyStats]) {
    if daily.is_empty() {
        return;
    }

    let oldest = daily.iter().map(|d| d.day.as_str()).min().unwrap_or("");
    let xlm_prices = match fetch_xlm_usd_by_day(oldest).await {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => {
            warn!("historical XLM/USD enrichment skipped: empty price map");
            HashMap::new()
        }
        Err(e) => {
            warn!(error = %e, "historical XLM/USD enrichment skipped");
            HashMap::new()
        }
    };

    for row in daily.iter_mut() {
        let xlm_px = xlm_prices.get(&row.day).copied();
        if let Some(px) = xlm_px {
            row.xlm_usd = Some(px);
        }

        let mut notional_usd = 0.0;
        let mut routed_usd = 0.0;
        let mut routed_priced_leg_count = 0u64;
        let mut gross_surplus_usd = 0.0;
        let mut notional_priced_any = false;
        let mut routed_priced_any = false;
        let mut surplus_priced_any = false;

        for tv in &row.by_token {
            let Some(usd_per_token) = usd_price_for_token(&tv.token, xlm_px) else {
                continue;
            };
            if tv.amount_in != 0 {
                notional_priced_any = true;
                notional_usd += (tv.amount_in as f64 / TOKEN_DECIMALS) * usd_per_token;
            }
        }
        for tv in &row.routed_by_token {
            let Some(usd_per_token) = usd_price_for_token(&tv.token, xlm_px) else {
                continue;
            };
            routed_priced_any = true;
            routed_usd += (tv.routed_volume as f64 / TOKEN_DECIMALS) * usd_per_token;
            routed_priced_leg_count += tv.routed_leg_count;
        }

        if notional_priced_any {
            row.total_amount_in_usd = Some(notional_usd);
        }
        if routed_priced_any {
            row.total_routed_dex_volume_usd = Some(routed_usd);
        }
        row.routed_priced_leg_count = routed_priced_leg_count;
        if row.routed_leg_count > 0 {
            row.routed_pricing_coverage = Some(routed_priced_leg_count as f64 / row.routed_leg_count as f64);
        }

        for surplus in &mut row.round_trip_by_token {
            let Some(usd_per_token) = usd_price_for_token(&surplus.base_token, xlm_px) else {
                continue;
            };
            let value = (surplus.gross_surplus as f64 / TOKEN_DECIMALS) * usd_per_token;
            surplus.gross_surplus_usd = Some(value);
            gross_surplus_usd += value;
            surplus_priced_any = true;
        }
        if surplus_priced_any {
            row.round_trip_gross_surplus_usd = Some(gross_surplus_usd);
        }
    }
}

fn usd_price_for_token(token: &str, xlm_usd: Option<f64>) -> Option<f64> {
    if token == XLM_SAC {
        return xlm_usd;
    }
    if token == USDC_SAC {
        return Some(1.0);
    }
    // Unknown / non-USD tokens (e.g. EURC): skip rather than mis-label as XLM.
    None
}

async fn fetch_xlm_usd_by_day(oldest_day: &str) -> Result<HashMap<String, f64>, String> {
    match fetch_coinpaprika_daily(oldest_day).await {
        Ok(m) if !m.is_empty() => Ok(m),
        Ok(_) => fetch_coingecko_daily(oldest_day).await,
        Err(e1) => match fetch_coingecko_daily(oldest_day).await {
            Ok(m) => Ok(m),
            Err(e2) => Err(format!("paprika: {e1}; coingecko: {e2}")),
        },
    }
}

#[derive(Debug, Deserialize)]
struct PaprikaPoint {
    timestamp: String,
    price: f64,
}

async fn fetch_coinpaprika_daily(oldest_day: &str) -> Result<HashMap<String, f64>, String> {
    let start = if oldest_day.len() >= 10 {
        &oldest_day[0..10]
    } else {
        "2020-01-01"
    };
    let url = format!("https://api.coinpaprika.com/v1/tickers/xlm-stellar/historical?start={start}&interval=1d");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "LumAgg/1.0 (+https://lumagg.xyz)")
        .send()
        .await
        .map_err(|e| format!("CoinPaprika request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("CoinPaprika HTTP {}", resp.status()));
    }
    let points: Vec<PaprikaPoint> = resp.json().await.map_err(|e| format!("CoinPaprika JSON: {e}"))?;

    let mut out = HashMap::new();
    for p in points {
        if !(p.price.is_finite() && p.price > 0.0) {
            continue;
        }
        let day = p.timestamp.get(0..10).unwrap_or("").to_string();
        if day.len() == 10 {
            out.insert(day, p.price);
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct MarketChart {
    prices: Vec<(f64, f64)>,
}

async fn fetch_coingecko_daily(oldest_day: &str) -> Result<HashMap<String, f64>, String> {
    let days = lookback_days_from_oldest(oldest_day).unwrap_or(14);
    let url = format!("https://api.coingecko.com/api/v3/coins/stellar/market_chart?vs_currency=usd&days={days}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "LumAgg/1.0 (+https://lumagg.xyz)")
        .send()
        .await
        .map_err(|e| format!("CoinGecko request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("CoinGecko HTTP {}", resp.status()));
    }
    let chart: MarketChart = resp.json().await.map_err(|e| format!("CoinGecko JSON: {e}"))?;

    let mut buckets: HashMap<String, (f64, u32)> = HashMap::new();
    for (ts_ms, price) in chart.prices {
        if !(price.is_finite() && price > 0.0) {
            continue;
        }
        let secs = (ts_ms / 1000.0) as i64;
        let day = unix_to_utc_day(secs);
        let entry = buckets.entry(day).or_insert((0.0, 0));
        entry.0 += price;
        entry.1 += 1;
    }

    Ok(buckets
        .into_iter()
        .filter_map(
            |(day, (sum, n))| {
                if n == 0 {
                    None
                } else {
                    Some((day, sum / f64::from(n)))
                }
            },
        )
        .collect())
}

fn unix_to_utc_day(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant civil_from_days (UTC midnight epoch days since 1970-01-01).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn lookback_days_from_oldest(oldest: &str) -> Option<u32> {
    if oldest.len() < 10 {
        return None;
    }
    let y: i32 = oldest[0..4].parse().ok()?;
    let m: u32 = oldest[5..7].parse().ok()?;
    let d: u32 = oldest[8..10].parse().ok()?;
    let oldest_days = days_from_civil(y, m, d)?;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let today_days = now_secs.div_euclid(86_400);
    let span = (today_days - oldest_days + 2).clamp(1, 90) as u32;
    Some(span.max(7))
}

fn days_from_civil(y: i32, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = y as i64;
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    Some(era * 146_097 + doe as i64 - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_epoch_day() {
        assert_eq!(unix_to_utc_day(0), "1970-01-01");
        let day = days_from_civil(2026, 7, 13).unwrap();
        assert_eq!(unix_to_utc_day(day * 86_400), "2026-07-13");
    }

    #[test]
    fn lookback_spans() {
        assert!(lookback_days_from_oldest("2026-07-13").unwrap() >= 7);
    }
}

//! HTTP client for LumAgg quote-api (`GET /api/v1/quote`).

use {
    crate::{config::ArbConfig, invoke::ArbSwapStep},
    anyhow::{anyhow, Context, Result},
    router_engine::{OptimalRoute, Path, SubOrder, TokenId},
    serde::Deserialize,
    std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    },
};

const QUOTE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct QuoteApiClient {
    base_urls: Vec<String>,
    next: std::sync::Arc<AtomicUsize>,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
struct QuoteApiResponse {
    success: bool,
    data: Option<QuoteApiData>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteApiData {
    pub amount_in: String,
    pub expected_output: String,
    pub minimum_output: String,
    pub price_impact: f64,
    pub is_split: bool,
    pub sub_routes: Vec<QuoteApiSubRoute>,
    pub compute_time_ms: u64,
    #[serde(default)]
    pub snapshot_age_ms: Option<u64>,
    #[serde(default)]
    pub pool_state_age_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteApiSubRoute {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub dex_types: Vec<String>,
    pub in_indices: Vec<u32>,
    pub out_indices: Vec<u32>,
    pub amount_in: String,
    pub amount_out: String,
    pub percentage: f64,
}

/// One quoted leg with router amounts and pre-resolved swap steps from the API.
#[derive(Debug, Clone)]
pub struct LegQuote {
    pub route: OptimalRoute,
    pub step_sets: Vec<Vec<ArbSwapStep>>,
    pub minimum_out: u128,
    pub snapshot_age_ms: Option<u64>,
    pub pool_state_age_ms: Option<u64>,
}

impl QuoteApiClient {
    pub fn new(base_urls: Vec<String>) -> Self {
        let base_urls: Vec<String> = if base_urls.is_empty() {
            vec!["http://127.0.0.1:3100".into()]
        } else {
            base_urls
                .into_iter()
                .map(|u| u.trim_end_matches('/').to_string())
                .collect()
        };
        Self {
            base_urls,
            next: std::sync::Arc::new(AtomicUsize::new(0)),
            http: reqwest::Client::new(),
        }
    }

    pub fn from_config(config: &ArbConfig) -> Self {
        Self::new(config.quote_api_urls.clone())
    }

    fn next_base_url(&self) -> &str {
        let i = self.next.fetch_add(1, Ordering::Relaxed);
        &self.base_urls[i % self.base_urls.len()]
    }

    /// Quote one directed leg via quote-api (path find + hydrate + local math).
    pub async fn quote_leg(
        &self,
        config: &ArbConfig,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
    ) -> Result<LegQuote> {
        self.quote_leg_with_validation(config, token_in, token_out, amount_in, config.on_chain_validate)
            .await
    }

    /// Quote one leg with an explicit validation mode. Execution can enable
    /// validation for one selected opportunity without slowing the scanner.
    pub async fn quote_leg_with_validation(
        &self,
        config: &ArbConfig,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        on_chain_validate: bool,
    ) -> Result<LegQuote> {
        if amount_in == 0 {
            return Err(anyhow!("amount_in must be positive"));
        }

        let slippage_pct = config.slippage_bps as f64 / 100.0;
        let base_url = self.next_base_url();
        let mut url = format!(
            "{base_url}/api/v1/quote?token_in={}&token_out={}&amount_in={}&slippage={slippage_pct}&prefer_soroban=1&max_hops={}&max_splits={}",
            token_in.canonical(),
            token_out.canonical(),
            amount_in,
            config.max_hops,
            config.max_splits,
        );
        if on_chain_validate {
            url.push_str("&on_chain_validate=1");
        }

        let resp = self
            .http
            .get(&url)
            .timeout(QUOTE_REQUEST_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        let status = resp.status();
        let body: QuoteApiResponse = resp
            .json()
            .await
            .with_context(|| format!("parse quote-api JSON (HTTP {status})"))?;

        if !body.success {
            return Err(anyhow!(
                "quote-api error for {} -> {} amount_in={amount_in}: {}",
                token_in.canonical(),
                token_out.canonical(),
                body.error.unwrap_or_else(|| "unknown".into())
            ));
        }

        let data = body
            .data
            .ok_or_else(|| anyhow!("quote-api returned success without data"))?;

        let leg = leg_quote_from_api(data)?;
        if leg.route.total_amount_in != amount_in {
            return Err(anyhow!(
                "quote-api amount_in mismatch: requested {amount_in}, returned {}",
                leg.route.total_amount_in
            ));
        }
        for sub in &leg.route.sub_orders {
            let first = sub.path.tokens.first().map(TokenId::canonical);
            let last = sub.path.tokens.last().map(TokenId::canonical);
            if first.as_deref() != Some(token_in.canonical().as_str()) ||
                last.as_deref() != Some(token_out.canonical().as_str())
            {
                return Err(anyhow!("quote-api sub_route token endpoints mismatch"));
            }
        }
        Ok(leg)
    }

    /// Lightweight expected-out quote (no hop metadata required by callers).
    pub async fn quote_expected_output(&self, token_in: &str, token_out: &str, amount_in: u128) -> Result<u128> {
        if amount_in == 0 {
            return Err(anyhow!("amount_in must be positive"));
        }
        let base_url = self.next_base_url();
        let url = format!(
            "{base_url}/api/v1/quote?token_in={token_in}&token_out={token_out}&amount_in={amount_in}&slippage=0.5&prefer_soroban=1&max_hops=3&max_splits=1"
        );
        let resp = self
            .http
            .get(&url)
            .timeout(QUOTE_REQUEST_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let body: QuoteApiResponse = resp
            .json()
            .await
            .with_context(|| format!("parse quote-api JSON (HTTP {status})"))?;
        if !body.success {
            return Err(anyhow!(
                "quote-api error for {token_in} -> {token_out}: {}",
                body.error.unwrap_or_else(|| "unknown".into())
            ));
        }
        let data = body
            .data
            .ok_or_else(|| anyhow!("quote-api returned success without data"))?;
        parse_u128_field("expected_output", &data.expected_output)
    }
}

fn parse_u128_field(name: &str, raw: &str) -> Result<u128> {
    raw.parse()
        .with_context(|| format!("invalid {name} in quote-api response: {raw}"))
}

pub fn leg_quote_from_api(data: QuoteApiData) -> Result<LegQuote> {
    if data.sub_routes.is_empty() {
        return Err(anyhow!("quote-api returned empty sub_routes"));
    }

    let mut sub_orders = Vec::with_capacity(data.sub_routes.len());
    let mut step_sets = Vec::with_capacity(data.sub_routes.len());

    for sub in &data.sub_routes {
        sub_orders.push(sub_order_from_api(sub)?);
        step_sets.push(steps_from_api_sub_route(sub)?);
    }

    let total_amount_in = parse_u128_field("amount_in", &data.amount_in)?;
    let total_expected_out = parse_u128_field("expected_output", &data.expected_output)?;
    let minimum_out = parse_u128_field("minimum_output", &data.minimum_output)?;
    let sub_amount_in = sub_orders.iter().try_fold(0u128, |total, sub| {
        total
            .checked_add(sub.amount_in)
            .ok_or_else(|| anyhow!("sub_route amount_in sum overflow"))
    })?;
    let sub_amount_out = sub_orders.iter().try_fold(0u128, |total, sub| {
        total
            .checked_add(sub.expected_amount_out)
            .ok_or_else(|| anyhow!("sub_route amount_out sum overflow"))
    })?;
    if sub_amount_in != total_amount_in || sub_amount_out != total_expected_out {
        return Err(anyhow!("quote-api sub_route totals do not match quote totals"));
    }
    if minimum_out > total_expected_out {
        return Err(anyhow!("quote-api minimum_output exceeds expected_output"));
    }

    Ok(LegQuote {
        route: OptimalRoute {
            sub_orders,
            total_amount_in,
            total_expected_out,
            price_impact_bps: (data.price_impact * 100.0).round() as u32,
            is_split: data.is_split,
            improvement_bps: 0,
            minimum_out,
            compute_time_ms: data.compute_time_ms,
            debug: None,
        },
        step_sets,
        minimum_out,
        snapshot_age_ms: data.snapshot_age_ms,
        pool_state_age_ms: data.pool_state_age_ms,
    })
}

fn sub_order_from_api(sub: &QuoteApiSubRoute) -> Result<SubOrder> {
    let hops = sub.pool_addresses.len();
    if sub.path.len() != hops + 1 {
        return Err(anyhow!(
            "quote-api sub_route path length {} != pool count + 1 ({hops})",
            sub.path.len()
        ));
    }
    if sub.dex_types.len() != hops || sub.in_indices.len() != hops || sub.out_indices.len() != hops {
        return Err(anyhow!("quote-api sub_route hop metadata length mismatch"));
    }

    Ok(SubOrder {
        path: Path {
            tokens: sub.path.iter().map(|t| TokenId::from_str_auto(t)).collect(),
            sources: sub.dex_types.clone(),
            pool_addresses: sub.pool_addresses.clone(),
            hops,
        },
        amount_in: parse_u128_field("sub_route.amount_in", &sub.amount_in)?,
        expected_amount_out: parse_u128_field("sub_route.amount_out", &sub.amount_out)?,
        fraction: sub.percentage / 100.0,
    })
}

fn steps_from_api_sub_route(sub: &QuoteApiSubRoute) -> Result<Vec<ArbSwapStep>> {
    let hops = sub.pool_addresses.len();
    let mut steps = Vec::with_capacity(hops);
    for i in 0..hops {
        let source = &sub.dex_types[i];
        let dex_type = crate::invoke::source_to_dex_type(source)?.to_string();
        steps.push(ArbSwapStep {
            venue_type: source.clone(),
            dex_type,
            pool_address: sub.pool_addresses[i].clone(),
            token_in: sub.path[i].clone(),
            token_out: sub.path[i + 1].clone(),
            in_idx: sub.in_indices[i],
            out_idx: sub.out_indices[i],
        });
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_cursor_is_shared_across_clones() {
        let client = QuoteApiClient::new(vec!["http://one".into(), "http://two".into()]);
        let clone = client.clone();
        assert_eq!(client.next_base_url(), "http://one");
        assert_eq!(clone.next_base_url(), "http://two");
        assert_eq!(client.next_base_url(), "http://one");
    }

    #[test]
    fn parses_api_sub_route() {
        let data = QuoteApiData {
            amount_in: "1000000000".into(),
            expected_output: "1001000000".into(),
            minimum_output: "1000500000".into(),
            price_impact: 0.12,
            is_split: false,
            compute_time_ms: 3,
            sub_routes: vec![QuoteApiSubRoute {
                source: "soroswap".into(),
                path: vec![
                    "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".into(),
                    "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".into(),
                ],
                pool_addresses: vec!["POOL123".into()],
                dex_types: vec!["soroswap".into()],
                in_indices: vec![0],
                out_indices: vec![1],
                amount_in: "1000000000".into(),
                amount_out: "1001000000".into(),
                percentage: 100.0,
            }],
            snapshot_age_ms: Some(1_000),
            pool_state_age_ms: Some(250),
        };
        let leg = leg_quote_from_api(data).unwrap();
        assert_eq!(leg.route.total_amount_in, 1_000_000_000);
        assert_eq!(leg.step_sets.len(), 1);
        assert_eq!(leg.step_sets[0][0].in_idx, 0);
        assert_eq!(leg.snapshot_age_ms, Some(1_000));
        assert_eq!(leg.pool_state_age_ms, Some(250));
    }
}

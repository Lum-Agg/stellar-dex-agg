//! HTTP quote client and limit-price eligibility gate.

use {
    crate::{book::OpenOrder, limit::required_min_out},
    anyhow::{anyhow, Context, Result},
    serde::Deserialize,
};

#[derive(Clone)]
pub struct QuoteApiClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub expected_output: i128,
    pub minimum_output: i128,
    pub sub_routes: Vec<QuoteSubRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QuoteSubRoute {
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    pub dex_types: Vec<String>,
    pub in_indices: Vec<u32>,
    pub out_indices: Vec<u32>,
    pub amount_in: String,
    pub amount_out: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QuoteStep {
    pub dex_type: String,
    pub pool_address: String,
    pub token_in: String,
    pub token_out: String,
    pub in_idx: u32,
    pub out_idx: u32,
}

#[derive(Debug, Deserialize)]
struct QuoteApiResponse {
    success: bool,
    data: Option<QuoteApiData>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuoteApiData {
    expected_output: String,
    minimum_output: String,
    sub_routes: Vec<QuoteSubRoute>,
}

impl QuoteApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn fetch_quote(&self, token_in: &str, token_out: &str, amount_in: i128) -> Result<Quote> {
        if amount_in <= 0 {
            return Err(anyhow!("amount_in must be positive"));
        }
        let url = format!("{}/api/v1/quote", self.base_url);
        let response = self
            .http
            .get(&url)
            .query(&[
                ("token_in", token_in),
                ("token_out", token_out),
                ("amount_in", &amount_in.to_string()),
            ])
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = response.status();
        let response: QuoteApiResponse = response
            .json()
            .await
            .with_context(|| format!("parse quote response (HTTP {status})"))?;
        if !response.success {
            return Err(anyhow!(
                "quote API rejected request: {}",
                response.error.unwrap_or_else(|| "unknown error".into())
            ));
        }
        let data = response
            .data
            .ok_or_else(|| anyhow!("quote API returned success without data"))?;
        quote_from_api_data(data)
    }
}

fn quote_from_api_data(data: QuoteApiData) -> Result<Quote> {
    Ok(Quote {
        expected_output: data.expected_output.parse().context("parse quote expected_output")?,
        minimum_output: data.minimum_output.parse().context("parse quote minimum_output")?,
        sub_routes: data.sub_routes,
    })
}

pub fn steps_from_api_sub_route(route: &QuoteSubRoute) -> Result<Vec<QuoteStep>> {
    let hops = route.pool_addresses.len();
    if route.path.len() != hops + 1 {
        return Err(anyhow!(
            "quote-api sub-route path length {} != pool count + 1 ({hops})",
            route.path.len()
        ));
    }
    if route.dex_types.len() != hops || route.in_indices.len() != hops || route.out_indices.len() != hops {
        return Err(anyhow!("quote-api sub-route hop metadata length mismatch"));
    }

    (0..hops)
        .map(|i| {
            Ok(QuoteStep {
                dex_type: source_to_dex_type(&route.dex_types[i])?.to_string(),
                pool_address: route.pool_addresses[i].clone(),
                token_in: route.path[i].clone(),
                token_out: route.path[i + 1].clone(),
                in_idx: route.in_indices[i],
                out_idx: route.out_indices[i],
            })
        })
        .collect()
}

fn source_to_dex_type(source: &str) -> Result<&'static str> {
    match source {
        "aquarius" | "aquarius_clmm" => Ok("aquarius"),
        "soroswap" => Ok("soroswap"),
        "phoenix" => Ok("phoenix"),
        "sushi" => Ok("sushi"),
        "comet" => Ok("comet"),
        other => Err(anyhow!("unsupported dex source: {other}")),
    }
}

pub fn is_fillable(order: &OpenOrder, expected_out: i128) -> bool {
    is_fillable_for(order, order.amount_in_remaining, expected_out)
}

pub fn is_fillable_for(order: &OpenOrder, amount_in: i128, expected_out: i128) -> bool {
    order.limit_out_per_in_e7 == 0 || expected_out >= required_min_out(amount_in, order.limit_out_per_in_e7)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::book::{OpenOrder, OrderKind},
    };

    fn order() -> OpenOrder {
        OpenOrder {
            kind: OrderKind::Limit,
            order_id: 7,
            owner: "owner".into(),
            token_in: "token-in".into(),
            token_out: "token-out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 20_000_000,
            expires_ledger: 999,
            chunk_amount: None,
            next_executable_ledger: None,
        }
    }

    #[test]
    fn fillable_when_expected_output_meets_limit() {
        assert!(is_fillable(&order(), 1_000));
    }

    #[test]
    fn not_fillable_when_expected_output_is_below_limit() {
        assert!(!is_fillable(&order(), 999));
    }

    #[test]
    fn deserializes_live_api_sub_route_shape_and_builds_steps() {
        let response: QuoteApiResponse = serde_json::from_str(
            r#"{
                "success": true,
                "data": {
                    "expected_output": "995",
                    "minimum_output": "990",
                    "sub_routes": [{
                        "path": ["CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM", "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBCC2KM"],
                        "pool_addresses": ["CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCDVF"],
                        "dex_types": ["soroswap"],
                        "in_indices": [0],
                        "out_indices": [1],
                        "amount_in": "1000",
                        "amount_out": "995"
                    }]
                }
            }"#,
        )
        .unwrap();

        let quote = quote_from_api_data(response.data.unwrap()).unwrap();
        let steps = steps_from_api_sub_route(&quote.sub_routes[0]).unwrap();

        assert_eq!(quote.expected_output, 995);
        assert_eq!(
            steps,
            vec![QuoteStep {
                dex_type: "soroswap".into(),
                pool_address: "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCDVF".into(),
                token_in: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".into(),
                token_out: "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBCC2KM".into(),
                in_idx: 0,
                out_idx: 1,
            }]
        );
    }
}

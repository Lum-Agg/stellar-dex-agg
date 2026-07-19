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
        Ok(Quote {
            expected_output: data
                .expected_output
                .parse()
                .context("parse quote expected_output")?,
            minimum_output: data
                .minimum_output
                .parse()
                .context("parse quote minimum_output")?,
        })
    }
}

pub fn is_fillable(order: &OpenOrder, expected_out: i128) -> bool {
    expected_out >= required_min_out(order.amount_in_remaining, order.limit_out_per_in_e7)
}

#[cfg(test)]
mod tests {
    use {super::is_fillable, crate::book::OpenOrder};

    fn order() -> OpenOrder {
        OpenOrder {
            order_id: 7,
            owner: "owner".into(),
            token_in: "token-in".into(),
            token_out: "token-out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 20_000_000,
            expires_ledger: 999,
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
}

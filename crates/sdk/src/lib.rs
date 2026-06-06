//! Rust client SDK for the Stellar DEX Aggregator API.
//! The primary SDK is TypeScript (in /packages/sdk/); this is for integration
//! testing.

use {
    anyhow::Result,
    serde::{Deserialize, Serialize},
};

pub struct AggregatorClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
pub struct QuoteParams {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub slippage: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct QuoteResponse {
    pub success: bool,
    pub data: Option<QuoteData>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuoteData {
    pub expected_output: String,
    pub minimum_output: String,
    pub price_impact: f64,
    pub is_split: bool,
    pub compute_time_ms: u64,
}

impl AggregatorClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_quote(&self, params: &QuoteParams) -> Result<QuoteResponse> {
        let url = format!("{}/api/v1/quote", self.base_url);
        let resp = self.client.get(&url).query(params).send().await?;
        Ok(resp.json().await?)
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/v1/health", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.status().is_success())
    }
}

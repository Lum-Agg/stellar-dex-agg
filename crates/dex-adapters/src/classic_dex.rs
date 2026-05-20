//! Classic DEX adapter: Stellar native orderbook + liquidity pools.
//!
//! Uses Horizon API `/paths/strict-send` to get quotes from Stellar Core's
//! native routing engine. This covers both the orderbook and native LP pools.
//!
//! The Classic DEX is treated as a single "black box" liquidity source:
//! we don't control its internal routing, but we know what output it gives
//! for a given input, and we can include it in split optimization.
//!
//! Execution: PathPaymentStrictSend operation in the same transaction as
//! Soroban contract calls (Stellar supports mixed Classic + Soroban ops).

use crate::traits::*;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default public Horizon endpoint
const DEFAULT_HORIZON_URL: &str = "https://horizon.stellar.org";

/// Well-known assets for Classic DEX path finding
const CLASSIC_ASSETS: &[(&str, &str, &str)] = &[
    // (contract_address, asset_code_for_horizon, issuer_or_native)
    (
        "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        "native",
        "",
    ),
    (
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "USDC",
        "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
    ),
    (
        "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        "EURC",
        "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
    ),
];

pub struct ClassicDexAdapter {
    horizon_url: String,
    client: Client,
    /// Pairs we expose (generated from CLASSIC_ASSETS combinations)
    pairs: RwLock<Vec<AdapterTradingPair>>,
}

impl ClassicDexAdapter {
    pub fn new(horizon_url: Option<&str>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            horizon_url: horizon_url.unwrap_or(DEFAULT_HORIZON_URL).to_string(),
            client,
            pairs: RwLock::new(Vec::new()),
        }
    }

    /// Convert a contract address to Horizon API asset parameters.
    fn contract_to_horizon_asset(contract: &str) -> Option<HorizonAsset> {
        for (addr, code, issuer) in CLASSIC_ASSETS {
            if *addr == contract {
                if *code == "native" {
                    return Some(HorizonAsset::Native);
                } else {
                    return Some(HorizonAsset::Credit {
                        code: code.to_string(),
                        issuer: issuer.to_string(),
                    });
                }
            }
        }
        None
    }

    /// Query Horizon `/paths/strict-send` for a quote.
    async fn horizon_quote(
        &self,
        source_asset: &HorizonAsset,
        dest_asset: &HorizonAsset,
        amount: u128,
    ) -> Result<Option<u128>> {
        // Convert amount from stroops to decimal string (7 decimals)
        let whole = amount / 10_000_000;
        let frac = amount % 10_000_000;
        let amount_str = format!("{}.{:07}", whole, frac);

        // Build destination_assets param (compact format: CODE:ISSUER or native)
        let dest_str = match dest_asset {
            HorizonAsset::Native => "native".to_string(),
            HorizonAsset::Credit { code, issuer } => format!("{}:{}", code, issuer),
        };

        // Build source asset params
        let source_params = match source_asset {
            HorizonAsset::Native => "source_asset_type=native".to_string(),
            HorizonAsset::Credit { code, issuer } => {
                let asset_type = if code.len() <= 4 {
                    "credit_alphanum4"
                } else {
                    "credit_alphanum12"
                };
                format!(
                    "source_asset_type={}&source_asset_code={}&source_asset_issuer={}",
                    asset_type, code, issuer
                )
            }
        };

        let full_url = format!(
            "{}/paths/strict-send?{}&source_amount={}&destination_assets={}",
            self.horizon_url, source_params, amount_str, dest_str
        );

        debug!("Horizon path query: {}", full_url);

        let resp = self.client.get(&full_url).send().await?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let body: serde_json::Value = resp.json().await?;

        // Parse response: _embedded.records[0].destination_amount
        let records = body
            .get("_embedded")
            .and_then(|e| e.get("records"))
            .and_then(|r| r.as_array());

        if let Some(records) = records {
            if let Some(first) = records.first() {
                if let Some(dest_amount_str) =
                    first.get("destination_amount").and_then(|v| v.as_str())
                {
                    let amount_out = parse_stellar_amount(dest_amount_str)?;
                    return Ok(Some(amount_out));
                }
            }
        }

        Ok(None)
    }

    /// Generate trading pairs from well-known assets.
    fn generate_pairs() -> Vec<AdapterTradingPair> {
        let mut pairs = Vec::new();

        for i in 0..CLASSIC_ASSETS.len() {
            for j in (i + 1)..CLASSIC_ASSETS.len() {
                let (addr_a, _, _) = CLASSIC_ASSETS[i];
                let (addr_b, _, _) = CLASSIC_ASSETS[j];

                pairs.push(AdapterTradingPair {
                    token_a: TokenId::Contract {
                        address: addr_a.to_string(),
                    },
                    token_b: TokenId::Contract {
                        address: addr_b.to_string(),
                    },
                    pool_address: format!("classic:{}:{}", addr_a, addr_b),
                    fee_bps: 0,      // Horizon handles fees internally
                    reserve_a: None, // Not applicable for black-box quotes
                    reserve_b: None,
                });
            }
        }

        pairs
    }
}

#[derive(Debug, Clone)]
enum HorizonAsset {
    Native,
    Credit { code: String, issuer: String },
}

#[derive(Debug, Deserialize)]
struct HorizonPathsResponse {
    #[serde(rename = "_embedded")]
    embedded: Option<HorizonEmbedded>,
    #[serde(default)]
    records: Vec<HorizonPathRecord>,
}

#[derive(Debug, Deserialize)]
struct HorizonEmbedded {
    records: Vec<HorizonPathRecord>,
}

#[derive(Debug, Deserialize)]
struct HorizonPathRecord {
    destination_amount: String,
    source_amount: String,
}

// Custom deserialize: Horizon wraps records in _embedded
impl HorizonPathsResponse {
    fn records(&self) -> &[HorizonPathRecord] {
        if let Some(embedded) = &self.embedded {
            &embedded.records
        } else {
            &self.records
        }
    }
}

/// Parse Stellar amount string (e.g., "15.1355107") to stroops (u128).
fn parse_stellar_amount(s: &str) -> Result<u128> {
    let parts: Vec<&str> = s.split('.').collect();
    let whole: u128 = parts[0].parse().unwrap_or(0);
    let frac: u128 = if parts.len() > 1 {
        let frac_str = format!("{:0<7}", &parts[1][..parts[1].len().min(7)]);
        frac_str.parse().unwrap_or(0)
    } else {
        0
    };
    Ok(whole * 10_000_000 + frac)
}

#[async_trait]
impl DexAdapter for ClassicDexAdapter {
    fn id(&self) -> &str {
        "classic_dex"
    }

    fn name(&self) -> &str {
        "Stellar Classic DEX"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::ClassicDex
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let pairs = Self::generate_pairs();
        *self.pairs.write().await = pairs.clone();
        info!("Classic DEX: {} pairs available", pairs.len());
        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        _pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let contract_in = match token_in {
            TokenId::Contract { address } => address.as_str(),
            TokenId::Native => "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
            _ => return Ok(None),
        };
        let contract_out = match token_out {
            TokenId::Contract { address } => address.as_str(),
            TokenId::Native => "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
            _ => return Ok(None),
        };

        let source_asset = match Self::contract_to_horizon_asset(contract_in) {
            Some(a) => a,
            None => return Ok(None),
        };
        let dest_asset = match Self::contract_to_horizon_asset(contract_out) {
            Some(a) => a,
            None => return Ok(None),
        };

        match self
            .horizon_quote(&source_asset, &dest_asset, amount_in)
            .await
        {
            Ok(Some(amount_out)) => {
                Ok(Some(AdapterQuote {
                    amount_out,
                    fee_bps: 0, // fees are baked into the output
                    // Estimate impact: assume ~$1M total liquidity for XLM/USDC on classic DEX
                    // impact ≈ amount_in / (2 * total_liquidity_in_stroops)
                    price_impact_bps: (amount_in / 2_000_000_0000000).min(10_000) as u32,
                }))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                debug!("Classic DEX quote failed: {}", e);
                Ok(None)
            }
        }
    }

    async fn build_swap_op(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        min_amount_out: u128,
        _pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::ClassicPathPayment {
            send_asset: token_in.canonical(),
            dest_asset: token_out.canonical(),
            send_amount: amount_in as i64,
            dest_min: min_amount_out as i64,
            path: vec![], // Stellar Core finds the path
        })
    }

    async fn health_check(&self) -> bool {
        let url = format!("{}/", self.horizon_url);
        self.client.get(&url).send().await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stellar_amount() {
        assert_eq!(parse_stellar_amount("15.1355107").unwrap(), 151355107);
        assert_eq!(parse_stellar_amount("100.0000000").unwrap(), 1000000000);
        assert_eq!(parse_stellar_amount("0.0000001").unwrap(), 1);
        assert_eq!(parse_stellar_amount("1000").unwrap(), 10000000000);
    }

    #[test]
    fn test_generate_pairs() {
        let pairs = ClassicDexAdapter::generate_pairs();
        // 3 assets → 3 pairs (3 choose 2)
        assert_eq!(pairs.len(), 3);
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_horizon_xlm_usdc_quote() {
        let adapter = ClassicDexAdapter::new(None);

        let xlm = TokenId::Contract {
            address: "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string(),
        };
        let usdc = TokenId::Contract {
            address: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".to_string(),
        };

        // Quote 100 XLM → USDC
        let quote = adapter
            .get_quote(&xlm, &usdc, 1_000_000_000, "classic:xlm:usdc")
            .await
            .unwrap();

        if let Some(q) = quote {
            println!("100 XLM → {} USDC (stroops)", q.amount_out);
            assert!(q.amount_out > 0);
        } else {
            println!("No Classic DEX path found (Horizon may be rate-limited)");
        }
    }
}

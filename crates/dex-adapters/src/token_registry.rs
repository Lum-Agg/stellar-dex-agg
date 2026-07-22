//! Token registry: maps between human-readable asset identifiers and contract
//! addresses.
//!
//! Stellar has three token formats:
//! - "native" → XLM (SAC:
//!   CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA)
//! - "CODE:ISSUER" → Classic asset (has a deterministic SAC address)
//! - "C..." → Soroban contract address (may be a SAC or a pure Soroban token)
//!
//! The registry resolves contract addresses to human-readable names by calling
//! the token's name() function, and caches the results.

use {
    crate::rpc::{scval_to_string, SorobanRpc},
    anyhow::Result,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
    tracing::debug,
};

/// Well-known tokens on Stellar mainnet (contract address → metadata).
/// These are pre-populated to avoid RPC calls for common tokens.
const WELL_KNOWN_TOKENS: &[(&str, &str, &str, u32)] = &[
    // (contract_address, symbol, name/asset_id, decimals)
    (
        "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        "XLM",
        "native",
        7,
    ),
    (
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "USDC",
        "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        7,
    ),
    (
        "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
        "EURC",
        "EURC:GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
        7,
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    /// Contract address (C...)
    pub contract: String,
    /// Human-readable symbol (e.g., "XLM", "USDC")
    pub symbol: String,
    /// Asset identifier: "native", "CODE:ISSUER", or contract address
    pub asset_id: String,
    /// Decimal places
    pub decimals: u32,
}

pub struct TokenRegistry {
    rpc: Arc<SorobanRpc>,
    /// contract_address -> TokenMeta
    cache: RwLock<HashMap<String, TokenMeta>>,
}

impl TokenRegistry {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        let mut cache = HashMap::new();

        // Pre-populate well-known tokens
        for (contract, symbol, asset_id, decimals) in WELL_KNOWN_TOKENS {
            cache.insert(
                contract.to_string(),
                TokenMeta {
                    contract: contract.to_string(),
                    symbol: symbol.to_string(),
                    asset_id: asset_id.to_string(),
                    decimals: *decimals,
                },
            );
        }

        Self {
            rpc,
            cache: RwLock::new(cache),
        }
    }

    /// Resolve a contract address to token metadata.
    /// Uses cache first, then RPC if not found.
    pub async fn resolve(&self, contract_address: &str) -> Option<TokenMeta> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(meta) = cache.get(contract_address) {
                return Some(meta.clone());
            }
        }

        // Fetch from chain
        match self.fetch_token_meta(contract_address).await {
            Ok(meta) => {
                let mut cache = self.cache.write().await;
                cache.insert(contract_address.to_string(), meta.clone());
                Some(meta)
            }
            Err(_) => None,
        }
    }

    /// Batch resolve multiple contract addresses.
    /// Returns a map of contract_address -> TokenMeta for those that resolved.
    pub async fn resolve_batch(&self, addresses: &[&str]) -> HashMap<String, TokenMeta> {
        let mut results = HashMap::new();
        let mut to_fetch = Vec::new();

        // Check cache first
        {
            let cache = self.cache.read().await;
            for &addr in addresses {
                if let Some(meta) = cache.get(addr) {
                    results.insert(addr.to_string(), meta.clone());
                } else {
                    to_fetch.push(addr.to_string());
                }
            }
        }

        // Fetch remaining in parallel (batches of 10)
        for chunk in to_fetch.chunks(10) {
            let futures: Vec<_> = chunk.iter().map(|addr| self.fetch_token_meta(addr)).collect();

            let fetch_results = futures::future::join_all(futures).await;

            let mut cache = self.cache.write().await;
            for (addr, result) in chunk.iter().zip(fetch_results) {
                if let Ok(meta) = result {
                    cache.insert(addr.to_string(), meta.clone());
                    results.insert(addr.to_string(), meta);
                }
            }
        }

        results
    }

    /// Get all cached tokens.
    pub async fn all_tokens(&self) -> Vec<TokenMeta> {
        self.cache.read().await.values().cloned().collect()
    }

    /// Fetch token metadata from chain via name() and symbol() calls.
    async fn fetch_token_meta(&self, contract_address: &str) -> Result<TokenMeta> {
        let (name_result, symbol_result) = tokio::join!(
            self.rpc.call_no_args(contract_address, "name"),
            self.rpc.call_no_args(contract_address, "symbol"),
        );

        let name = name_result
            .ok()
            .and_then(|v| scval_to_string(&v).ok())
            .unwrap_or_default();

        let symbol = symbol_result
            .ok()
            .and_then(|v| scval_to_string(&v).ok())
            .unwrap_or_else(|| contract_address[..6].to_string());

        // Determine asset_id from name
        let asset_id = if name == "native" || name.contains(':') {
            name.clone()
        } else {
            contract_address.to_string()
        };

        debug!(contract = contract_address, symbol = %symbol, asset_id = %asset_id, "Resolved token");

        Ok(TokenMeta {
            contract: contract_address.to_string(),
            symbol,
            asset_id,
            decimals: 7, // Default for Stellar
        })
    }
}

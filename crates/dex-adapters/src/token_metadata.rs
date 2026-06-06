//! Token metadata cache: persists token symbol/name to a JSON file.
//! On startup, loads from file. In background, resolves unknown tokens via RPC.

use {
    crate::rpc::{scval_to_string, SorobanRpc},
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

const METADATA_FILE: &str = "data/token_metadata.json";

fn logo_url_for_asset_id(asset_id: &str) -> Option<String> {
    if asset_id == "native" {
        return Some("https://stellar.expert/explorer/public/asset/native/icon".to_string());
    }
    if let Some((code, issuer)) = asset_id.split_once(':') {
        if !code.is_empty() && !issuer.is_empty() {
            return Some(format!(
                "https://stellar.expert/explorer/public/asset/{}-{}-1/icon",
                code, issuer
            ));
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub contract: String,
    pub symbol: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataCache {
    tokens: HashMap<String, TokenMetadata>,
}

pub struct TokenMetadataStore {
    cache: Arc<RwLock<HashMap<String, TokenMetadata>>>,
    rpc: Arc<SorobanRpc>,
}

impl TokenMetadataStore {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        let mut cache = HashMap::new();

        // Load from file
        if let Ok(data) = std::fs::read_to_string(METADATA_FILE) {
            if let Ok(file_cache) = serde_json::from_str::<MetadataCache>(&data) {
                cache = file_cache.tokens;
                info!("Loaded {} token metadata entries from cache", cache.len());
            }
        }

        Self {
            cache: Arc::new(RwLock::new(cache)),
            rpc,
        }
    }

    /// Get metadata for a token (returns None if not yet resolved).
    pub async fn get(&self, contract: &str) -> Option<TokenMetadata> {
        self.cache.read().await.get(contract).cloned()
    }

    /// Get all cached metadata.
    pub async fn get_all(&self) -> HashMap<String, TokenMetadata> {
        self.cache.read().await.clone()
    }

    /// Replace the cache contents with a prebuilt snapshot.
    pub async fn replace_all(&self, tokens: HashMap<String, TokenMetadata>) {
        *self.cache.write().await = tokens;
    }

    /// Resolve unknown tokens in the background.
    /// Call this with a list of all known token addresses.
    pub async fn resolve_unknown(&self, token_addresses: Vec<String>) {
        let cache = self.cache.read().await;
        let unknown: Vec<String> = token_addresses
            .into_iter()
            .filter(|addr| !cache.contains_key(addr))
            .collect();
        drop(cache);

        if unknown.is_empty() {
            return;
        }

        info!("Resolving metadata for {} unknown tokens...", unknown.len());

        let mut resolved = 0;
        for addr in &unknown {
            match self.fetch_token_metadata(addr).await {
                Some(meta) => {
                    self.cache.write().await.insert(addr.clone(), meta);
                    resolved += 1;
                }
                None => {
                    // Store with contract prefix as symbol so we don't retry
                    let short = if addr.len() > 8 { &addr[..8] } else { addr.as_str() };
                    self.cache.write().await.insert(
                        addr.clone(),
                        TokenMetadata {
                            contract: addr.clone(),
                            symbol: short.to_string(),
                            name: "Unknown".to_string(),
                            logo: None,
                        },
                    );
                }
            }

            // Rate limit: don't hammer the RPC
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        info!("Resolved {}/{} token metadata", resolved, unknown.len());

        // Save to file
        self.save().await;
    }

    /// Fetch symbol and name from chain via simulate_call.
    async fn fetch_token_metadata(&self, contract: &str) -> Option<TokenMetadata> {
        // Use public RPC for metadata (more reliable than local for some contracts)
        let public_rpc = SorobanRpc::new(
            "https://soroban-rpc.mainnet.stellar.gateway.fm",
            "Public Global Stellar Network ; September 2015",
        );

        // Call symbol()
        let symbol = match public_rpc.call_no_args(contract, "symbol").await {
            Ok(val) => scval_to_string(&val).ok().unwrap_or_default(),
            Err(_) => return None,
        };

        // Call name()
        let name = match public_rpc.call_no_args(contract, "name").await {
            Ok(val) => scval_to_string(&val).ok().unwrap_or_default(),
            Err(_) => symbol.clone(),
        };

        if symbol.is_empty() {
            return None;
        }

        // For SAC tokens, name is "CODE:ISSUER" — use code as display name
        let asset_id = if name.contains(':') || name == "native" {
            name.clone()
        } else {
            contract.to_string()
        };
        let display_name = if name.contains(':') {
            name.split(':').next().unwrap_or(&name).to_string()
        } else if name == "native" {
            "Stellar Lumens".to_string()
        } else {
            name.clone()
        };
        let logo = logo_url_for_asset_id(&asset_id);

        debug!(
            "Resolved token {}: symbol={}, name={}",
            &contract[..12.min(contract.len())],
            symbol,
            display_name
        );

        Some(TokenMetadata {
            contract: contract.to_string(),
            symbol,
            name: display_name,
            logo,
        })
    }

    /// Save cache to file.
    async fn save(&self) {
        let cache = self.cache.read().await;
        let file_cache = MetadataCache { tokens: cache.clone() };

        match serde_json::to_string_pretty(&file_cache) {
            Ok(json) => {
                if let Err(e) = std::fs::write(METADATA_FILE, json) {
                    warn!("Failed to save token metadata: {}", e);
                } else {
                    info!("Saved {} token metadata entries to cache", cache.len());
                }
            }
            Err(e) => warn!("Failed to serialize token metadata: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replace_all_overwrites_existing_cache() {
        let rpc = Arc::new(SorobanRpc::new(
            "https://soroban-rpc.mainnet.stellar.gateway.fm",
            "Public Global Stellar Network ; September 2015",
        ));
        let store = TokenMetadataStore::new(rpc);
        let mut replacement = HashMap::new();
        replacement.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
            },
        );

        store.replace_all(replacement).await;

        let all = store.get_all().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all["token-1"].symbol, "TOK");
    }
}

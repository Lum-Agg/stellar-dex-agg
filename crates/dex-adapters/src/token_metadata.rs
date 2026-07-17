//! Token metadata cache: persists token symbol/name to a JSON file.
//! On startup, loads from file. In background, resolves unknown tokens via RPC.

use {
    crate::{
        rpc::{scval_to_string, SorobanRpc},
        token_logo::TokenLogoCache,
    },
    serde::{Deserialize, Serialize},
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::Arc,
    },
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

fn load_metadata_file(path: &Path) -> HashMap<String, TokenMetadata> {
    match std::fs::read_to_string(path) {
        Ok(data) => match serde_json::from_str::<MetadataCache>(&data) {
            Ok(file_cache) => {
                info!("Loaded {} token metadata entries from cache", file_cache.tokens.len());
                file_cache.tokens
            }
            Err(_) => HashMap::new(),
        },
        Err(_) => HashMap::new(),
    }
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
    logo_cache: Arc<TokenLogoCache>,
    metadata_file: PathBuf,
}

impl TokenMetadataStore {
    pub fn new(_rpc: Arc<SorobanRpc>) -> Self {
        Self::with_logo_cache(TokenLogoCache::from_env())
    }

    /// Construct with an explicit logo cache, loading the default metadata file.
    pub fn with_logo_cache(logo_cache: TokenLogoCache) -> Self {
        let metadata_file = PathBuf::from(METADATA_FILE);
        let cache = load_metadata_file(&metadata_file);
        Self {
            cache: Arc::new(RwLock::new(cache)),
            logo_cache: Arc::new(logo_cache),
            metadata_file,
        }
    }

    /// Test helper: supply logo cache, metadata path, and initial entries
    /// without reading or writing the repository metadata file.
    #[cfg(test)]
    fn with_logo_cache_and_file(
        logo_cache: TokenLogoCache,
        metadata_file: impl Into<PathBuf>,
        initial: HashMap<String, TokenMetadata>,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(initial)),
            logo_cache: Arc::new(logo_cache),
            metadata_file: metadata_file.into(),
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

    /// Ensure every cached token has a self-hosted logo URL on disk.
    ///
    /// Clones entries before any await so the RwLock is never held across I/O.
    /// Returns the number of tokens that successfully received a self-hosted URL.
    pub async fn ensure_self_hosted_logos(&self) -> usize {
        let entries: Vec<(String, TokenMetadata)> = {
            let cache = self.cache.read().await;
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut success = 0usize;
        let mut updates: Vec<(String, String)> = Vec::new();

        for (id, meta) in entries {
            let remote = meta.logo.as_deref();
            match self.logo_cache.ensure_logo(&meta.contract, &meta.symbol, remote).await {
                Ok(url) => {
                    success += 1;
                    if meta.logo.as_deref() != Some(url.as_str()) {
                        updates.push((id, url));
                    }
                }
                Err(e) => {
                    warn!("Failed to ensure self-hosted logo for {}: {}", id, e);
                }
            }
        }

        if !updates.is_empty() {
            {
                let mut cache = self.cache.write().await;
                for (id, url) in updates {
                    if let Some(entry) = cache.get_mut(&id) {
                        entry.logo = Some(url);
                    }
                }
            }
            self.save().await;
        }

        success
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
            // Backfill self-hosted logos for already-cached entries.
            self.ensure_self_hosted_logos().await;
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

        // Persist newly resolved metadata (may still have third-party logo URLs).
        self.save().await;
        // Migrate logos to self-hosted URLs once for this resolve pass.
        self.ensure_self_hosted_logos().await;
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
                if let Some(parent) = self.metadata_file.parent() {
                    if !parent.as_os_str().is_empty() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            warn!("Failed to create token metadata directory: {}", e);
                            return;
                        }
                    }
                }
                if let Err(e) = std::fs::write(&self.metadata_file, json) {
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
    use crate::token_logo::TokenLogoCache;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dex-adapters-token-meta-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn store_with_initial(
        logo_dir: &std::path::Path,
        base_url: &str,
        metadata_file: PathBuf,
        initial: HashMap<String, TokenMetadata>,
    ) -> TokenMetadataStore {
        TokenMetadataStore::with_logo_cache_and_file(TokenLogoCache::new(logo_dir, base_url), metadata_file, initial)
    }

    #[tokio::test]
    async fn replace_all_overwrites_existing_cache() {
        let logo_dir = unique_temp_dir("replace-logos");
        let meta_file = unique_temp_dir("replace-meta").join("token_metadata.json");
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, HashMap::new());
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

    #[tokio::test]
    async fn enriches_missing_logo_with_self_hosted_fallback() {
        let logo_dir = unique_temp_dir("enrich-logos");
        let meta_file = unique_temp_dir("enrich-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file.clone(), initial);

        let count = store.ensure_self_hosted_logos().await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
        assert_eq!(count, 1);

        let persisted = std::fs::read_to_string(&meta_file).expect("metadata persisted");
        assert!(persisted.contains("https://api.test/logos/"));
    }

    #[tokio::test]
    async fn enriches_when_external_logo_download_fails() {
        let logo_dir = unique_temp_dir("fail-logos");
        let meta_file = unique_temp_dir("fail-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-2".to_string(),
            TokenMetadata {
                contract: "token-2".to_string(),
                symbol: "EXT".to_string(),
                name: "External".to_string(),
                logo: Some("https://127.0.0.1:1/missing-logo.png".to_string()),
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, initial);

        let count = store.ensure_self_hosted_logos().await;

        let meta = store.get("token-2").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn resolve_unknown_backfills_logos_when_no_unknown_tokens() {
        let logo_dir = unique_temp_dir("backfill-logos");
        let meta_file = unique_temp_dir("backfill-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, initial);

        store.resolve_unknown(vec!["token-1".to_string()]).await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
    }
}

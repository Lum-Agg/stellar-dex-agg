//! Token metadata cache: persists token symbol/name to a JSON file.
//! On startup, loads from file. In background, resolves unknown tokens via RPC.

use {
    crate::{
        rpc::{scval_to_string, SorobanRpc},
        token_logo::TokenLogoCache,
        token_logo_lists::TokenLogoListIndex,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogoKind {
    Official,
    #[default]
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub contract: String,
    pub symbol: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Distinguishes downloaded SEP-42 icons from generated letter avatars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_kind: Option<LogoKind>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataCache {
    tokens: HashMap<String, TokenMetadata>,
}

pub struct TokenMetadataStore {
    cache: Arc<RwLock<HashMap<String, TokenMetadata>>>,
    logo_cache: Arc<TokenLogoCache>,
    logo_lists: Arc<TokenLogoListIndex>,
    metadata_file: PathBuf,
}

impl TokenMetadataStore {
    pub fn new(_rpc: Arc<SorobanRpc>) -> Self {
        Self::with_logo_cache(TokenLogoCache::from_env(), TokenLogoListIndex::from_env())
    }

    /// Construct with an explicit logo cache and SEP-42 list index.
    pub fn with_logo_cache(logo_cache: TokenLogoCache, logo_lists: TokenLogoListIndex) -> Self {
        let metadata_file = PathBuf::from(METADATA_FILE);
        let cache = load_metadata_file(&metadata_file);
        Self {
            cache: Arc::new(RwLock::new(cache)),
            logo_cache: Arc::new(logo_cache),
            logo_lists: Arc::new(logo_lists),
            metadata_file,
        }
    }

    /// Test helper: supply logo cache, metadata path, and initial entries
    /// without reading or writing the repository metadata file.
    #[cfg(test)]
    fn with_logo_cache_and_file(
        logo_cache: TokenLogoCache,
        logo_lists: TokenLogoListIndex,
        metadata_file: impl Into<PathBuf>,
        initial: HashMap<String, TokenMetadata>,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(initial)),
            logo_cache: Arc::new(logo_cache),
            logo_lists: Arc::new(logo_lists),
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
    /// Prefers official icons from SEP-42 lists; falls back to generated SVG.
    /// Clones entries before any await so the RwLock is never held across I/O.
    /// Returns the number of tokens that successfully received a self-hosted
    /// URL.
    pub async fn ensure_self_hosted_logos(&self) -> usize {
        let list_count = self.logo_lists.refresh().await;
        info!(list_count, "Refreshed SEP-42 logo index");

        let entries: Vec<(String, TokenMetadata)> = {
            let cache = self.cache.read().await;
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut success = 0usize;
        let mut official = 0usize;
        let mut updates: Vec<(String, String, LogoKind)> = Vec::new();

        for (id, meta) in entries {
            let list_icon = self.logo_lists.icon_url(&meta.contract).await;
            // Prefer SEP-42 list icon over any stale third-party URL in metadata.
            let remote = list_icon
                .as_deref()
                .or(meta.logo.as_deref().filter(|u| !u.contains("/logos/")));

            match self.logo_cache.ensure_logo(&meta.contract, &meta.symbol, remote).await {
                Ok(url) => {
                    success += 1;
                    let kind = if self.logo_cache.has_official_cache(&meta.contract) {
                        official += 1;
                        LogoKind::Official
                    } else {
                        LogoKind::Fallback
                    };
                    if meta.logo.as_deref() != Some(url.as_str()) || meta.logo_kind != Some(kind) {
                        updates.push((id, url, kind));
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
                for (id, url, kind) in updates {
                    if let Some(entry) = cache.get_mut(&id) {
                        entry.logo = Some(url);
                        entry.logo_kind = Some(kind);
                    }
                }
            }
            self.save().await;
        }

        info!(success, official, "Self-hosted logo enrichment complete");
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
                            logo_kind: None,
                        },
                    );
                }
            }

            // Rate limit: don't hammer the RPC
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        info!("Resolved {}/{} token metadata", resolved, unknown.len());

        // Persist newly resolved metadata, then migrate logos to self-hosted URLs.
        self.save().await;
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
        let display_name = if name.contains(':') {
            name.split(':').next().unwrap_or(&name).to_string()
        } else if name == "native" {
            "Stellar Lumens".to_string()
        } else {
            name.clone()
        };

        // Official icons come from SEP-42 lists during ensure_self_hosted_logos.
        let logo = self.logo_lists.icon_url(contract).await;

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
            logo_kind: None,
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
    use {
        super::*,
        crate::{token_logo::TokenLogoCache, token_logo_lists::TokenLogoListIndex},
        std::{
            path::PathBuf,
            time::{SystemTime, UNIX_EPOCH},
        },
    };

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
        TokenMetadataStore::with_logo_cache_and_file(
            TokenLogoCache::new(logo_dir, base_url),
            TokenLogoListIndex::new(vec![]),
            metadata_file,
            initial,
        )
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
                logo_kind: None,
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
                logo_kind: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file.clone(), initial);

        let count = store.ensure_self_hosted_logos().await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(meta.logo_kind, Some(LogoKind::Fallback));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
        assert_eq!(count, 1);

        let persisted = std::fs::read_to_string(&meta_file).expect("metadata persisted");
        assert!(persisted.contains("https://api.test/logos/"));
        assert!(persisted.contains("fallback"));
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
                logo_kind: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, initial);

        let count = store.ensure_self_hosted_logos().await;

        let meta = store.get("token-2").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(meta.logo_kind, Some(LogoKind::Fallback));
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
                logo_kind: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, initial);

        store.resolve_unknown(vec!["token-1".to_string()]).await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn prefers_sep42_list_icon_over_stale_metadata_url() {
        let logo_dir = unique_temp_dir("sep42-logos");
        let meta_file = unique_temp_dir("sep42-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-sep".to_string(),
            TokenMetadata {
                contract: "token-sep".to_string(),
                symbol: "SEP".to_string(),
                name: "Sep".to_string(),
                logo: Some("https://stellar.expert/explorer/public/asset/native/icon".into()),
                logo_kind: None,
            },
        );

        let lists = TokenLogoListIndex::new(vec![]);
        lists
            .icons
            .write()
            .await
            .insert("token-sep".into(), "https://127.0.0.1:1/official-missing.png".into());

        let store = TokenMetadataStore::with_logo_cache_and_file(
            TokenLogoCache::new(&logo_dir, "https://api.test/logos"),
            lists,
            meta_file,
            initial,
        );

        let _ = store.ensure_self_hosted_logos().await;
        let meta = store.get("token-sep").await.expect("present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        // Download fails → fallback; important part is list URL was preferred.
        assert_eq!(meta.logo_kind, Some(LogoKind::Fallback));
    }
}

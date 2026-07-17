//! Filesystem-backed token logo cache with remote download and SVG fallback.

use {
    sha2::{Digest, Sha256},
    std::{
        env,
        path::{Path, PathBuf},
        time::Duration,
    },
};

const MAX_LOGO_BYTES: u64 = 1024 * 1024; // 1 MiB
const DEFAULT_DIR: &str = "data/logos";
const DEFAULT_BASE_URL: &str = "https://api.lumagg.xyz/logos";
const CACHED_EXTENSIONS: &[&str] = &["png", "jpg", "webp", "svg"];

pub struct TokenLogoCache {
    directory: PathBuf,
    base_url: String,
    client: reqwest::Client,
}

impl TokenLogoCache {
    pub fn from_env() -> Self {
        let directory = env::var("TOKEN_LOGO_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_string());
        let base_url = env::var("TOKEN_LOGO_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Self::new(directory, base_url)
    }

    pub fn new(directory: impl Into<PathBuf>, base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        Self {
            directory: directory.into(),
            base_url: base_url.into(),
            client,
        }
    }

    /// Deterministic fallback SVG path for `token_id` (SHA-256 hash; never embeds token text).
    pub fn fallback_path(&self, token_id: &str) -> PathBuf {
        self.path_for_ext(token_id, "svg")
    }

    pub async fn ensure_logo(&self, token_id: &str, symbol: &str, remote_url: Option<&str>) -> anyhow::Result<String> {
        if let Some(existing) = self.find_existing(token_id) {
            return Ok(self.url_for_path(&existing)?);
        }

        if let Some(url) = remote_url {
            if let Some((bytes, ext)) = self.try_download(url).await {
                let path = self.path_for_ext(token_id, ext);
                self.atomic_write(&path, &bytes)?;
                return Ok(self.url_for_path(&path)?);
            }
        }

        let svg = fallback_svg(symbol, token_id);
        let path = self.fallback_path(token_id);
        self.atomic_write(&path, svg.as_bytes())?;
        Ok(self.url_for_path(&path)?)
    }

    fn token_hash_hex(token_id: &str) -> String {
        hex::encode(Sha256::digest(token_id.as_bytes()))
    }

    fn path_for_ext(&self, token_id: &str, ext: &str) -> PathBuf {
        self.directory
            .join(format!("{}.{}", Self::token_hash_hex(token_id), ext))
    }

    fn find_existing(&self, token_id: &str) -> Option<PathBuf> {
        for ext in CACHED_EXTENSIONS {
            let path = self.path_for_ext(token_id, ext);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    fn url_for_path(&self, path: &Path) -> anyhow::Result<String> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid logo filename"))?;
        let base = self.base_url.trim_end_matches('/');
        Ok(format!("{base}/{filename}"))
    }

    async fn try_download(&self, url: &str) -> Option<(Vec<u8>, &'static str)> {
        let response = self.client.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        if let Some(len) = response.content_length() {
            if len > MAX_LOGO_BYTES {
                return None;
            }
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let ext = extension_for_content_type(content_type)?;

        let bytes = response.bytes().await.ok()?;
        if bytes.len() as u64 > MAX_LOGO_BYTES {
            return None;
        }

        Some((bytes.to_vec(), ext))
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid logo path"))?;
        let tmp = path.with_file_name(format!(
            "{}.{}.{}.tmp",
            file_name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        if let Err(e) = std::fs::write(&tmp, data) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }

        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }

        Ok(())
    }
}

/// Map an HTTP Content-Type to a safe raster extension, if supported.
pub fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Deterministic SVG avatar with XML-escaped symbol text and hash-derived colors.
pub fn fallback_svg(symbol: &str, token_id: &str) -> String {
    let escaped = escape_xml(symbol);
    let (bg, fg) = colors_from_token(token_id);
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">"#,
            r#"<rect width="128" height="128" rx="64" fill="{bg}"/>"#,
            r#"<text x="64" y="64" dy="0.35em" text-anchor="middle" fill="{fg}" "#,
            r#"font-family="sans-serif" font-size="36" font-weight="600">{symbol}</text>"#,
            r#"</svg>"#
        ),
        bg = bg,
        fg = fg,
        symbol = escaped
    )
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn colors_from_token(token_id: &str) -> (String, &'static str) {
    let digest = Sha256::digest(token_id.as_bytes());
    let r = digest[0];
    let g = digest[1];
    let b = digest[2];
    let bg = format!("#{r:02x}{g:02x}{b:02x}");
    let luminance = (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
    let fg = if luminance > 0.55 { "#1a1a1a" } else { "#ffffff" };
    (bg, fg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_deterministic_and_safe() {
        let cache = TokenLogoCache::new("data/logos", "https://api.lumagg.xyz/logos");
        let first = cache.fallback_path("CA/unsafe:token");
        let second = cache.fallback_path("CA/unsafe:token");
        assert_eq!(first, second);
        assert!(!first.to_string_lossy().contains("unsafe"));
        assert_eq!(first.extension().and_then(|v| v.to_str()), Some("svg"));
    }

    #[test]
    fn fallback_svg_escapes_token_symbol() {
        let svg = fallback_svg("A<&", "CA123");
        assert!(svg.contains("A&lt;&amp;"));
        assert!(!svg.contains("A<&"));
    }

    #[test]
    fn only_supported_raster_content_types_are_accepted() {
        assert_eq!(extension_for_content_type("image/png"), Some("png"));
        assert_eq!(extension_for_content_type("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for_content_type("image/webp"), Some("webp"));
        assert_eq!(extension_for_content_type("image/svg+xml"), None);
        assert_eq!(extension_for_content_type("text/html"), None);
    }
}

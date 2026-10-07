//! Filesystem-backed token logo cache with remote download and SVG fallback.

use {
    image::{imageops::FilterType, ImageFormat},
    sha2::{Digest, Sha256},
    std::{
        env,
        io::Cursor,
        path::{Path, PathBuf},
        time::Duration,
    },
};

const MAX_LOGO_BYTES: u64 = 1024 * 1024; // 1 MiB
const MAX_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024; // enough to normalize oversized source art
const MAX_RASTER_DIMENSION: u32 = 512;
const DEFAULT_DIR: &str = "data/logos";
const DEFAULT_BASE_URL: &str = "https://api.lumagg.xyz/logos";
const CACHED_EXTENSIONS: &[&str] = &["png", "jpg", "webp", "gif", "svg"];

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

    /// Deterministic fallback SVG path for `token_id` (SHA-256 hash; never
    /// embeds token text).
    pub fn fallback_path(&self, token_id: &str) -> PathBuf {
        self.path_for_ext(token_id, "svg")
    }

    pub async fn ensure_logo(&self, token_id: &str, symbol: &str, remote_url: Option<&str>) -> anyhow::Result<String> {
        // Prefer downloading an official remote when provided. Raster caches
        // are treated as already-official and skip re-download; SVG caches may
        // be leftover fallbacks and are eligible for upgrade.
        if let Some(existing) = self.find_existing(token_id) {
            let ext = existing.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_raster = matches!(ext, "png" | "jpg" | "webp" | "gif");
            if is_raster || remote_url.is_none() {
                return Ok(self.url_for_path(&existing)?);
            }
        }

        if let Some(url) = remote_url {
            if let Some((bytes, ext)) = self.try_download(url).await {
                let path = self.path_for_ext(token_id, ext);
                self.atomic_write(&path, &bytes)?;
                // Drop stale SVG fallback if we just wrote a non-SVG official.
                if ext != "svg" {
                    let _ = std::fs::remove_file(self.path_for_ext(token_id, "svg"));
                }
                return Ok(self.url_for_path(&path)?);
            }
        }

        if let Some(existing) = self.find_existing(token_id) {
            return Ok(self.url_for_path(&existing)?);
        }

        let svg = fallback_svg(symbol, token_id);
        let path = self.fallback_path(token_id);
        self.atomic_write(&path, svg.as_bytes())?;
        Ok(self.url_for_path(&path)?)
    }

    /// True when the cached file for `token_id` looks like a downloaded raster
    /// (or non-fallback SVG). Used by metadata to label `logo_kind`.
    pub fn has_official_cache(&self, token_id: &str) -> bool {
        let Some(path) = self.find_existing(token_id) else {
            return false;
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if matches!(ext, "png" | "jpg" | "webp" | "gif") {
            return true;
        }
        if ext == "svg" {
            if let Ok(bytes) = std::fs::read(&path) {
                // Our generated fallback always includes this exact font-family marker.
                let text = String::from_utf8_lossy(&bytes);
                return !text.contains("font-family=\"sans-serif\" font-size=\"36\"");
            }
        }
        false
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
            if len > MAX_DOWNLOAD_BYTES {
                return None;
            }
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response.bytes().await.ok()?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return None;
        }

        // Trust the actual bytes, not the Content-Type header: some hosts
        // return HTML (e.g. an error/landing page) with an image content type.
        let ext = detect_image_ext(&bytes, &content_type)?;

        // Keep the original format; only SVG needs sanitizing before we self-host it.
        if ext == "svg" {
            let sanitized = sanitize_svg(&bytes)?;
            return Some((sanitized.into_bytes(), ext));
        }

        if bytes.len() as u64 > MAX_LOGO_BYTES {
            let normalized = normalize_oversized_raster(&bytes)?;
            return Some((normalized, "png"));
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

fn normalize_oversized_raster(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    for dimension in [MAX_RASTER_DIMENSION, MAX_RASTER_DIMENSION / 2] {
        let resized = image.resize(dimension, dimension, FilterType::Lanczos3);
        let mut output = Cursor::new(Vec::new());
        resized.write_to(&mut output, ImageFormat::Png).ok()?;
        if output.get_ref().len() as u64 <= MAX_LOGO_BYTES {
            return Some(output.into_inner());
        }
    }
    None
}

/// Map an HTTP Content-Type to a supported image extension, if known.
/// Prefer [`detect_image_ext`] on the response body; this is only a hint.
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
        "image/gif" => Some("gif"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

/// Detect image type from magic bytes (preferred) or Content-Type hint.
/// Rejects HTML/text so we never self-host error pages as logos.
pub fn detect_image_ext(bytes: &[u8], content_type: &str) -> Option<&'static str> {
    if looks_like_html(bytes) {
        return None;
    }

    if bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if looks_like_svg(bytes) {
        return Some("svg");
    }

    // Last resort: Content-Type hint only when bytes look binary and not HTML.
    extension_for_content_type(content_type).filter(|ext| *ext != "svg")
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let head = trim_leading_whitespace(bytes);
    let lower: Vec<u8> = head.iter().take(64).map(u8::to_ascii_lowercase).collect();
    lower.starts_with(b"<!doctype html")
        || lower.starts_with(b"<html")
        || lower.starts_with(b"<head")
        || lower.starts_with(b"<body")
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = trim_leading_whitespace(bytes);
    let lower: Vec<u8> = head.iter().take(256).map(u8::to_ascii_lowercase).collect();
    // Accept <?xml ...><svg or bare <svg
    if lower.starts_with(b"<svg") {
        return true;
    }
    if lower.starts_with(b"<?xml") {
        return contains_ascii_ci(&lower, b"<svg");
    }
    false
}

fn trim_leading_whitespace(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    &bytes[i..]
}

fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

/// Sanitize a remote SVG so it is safe to self-host.
///
/// Keeps the SVG format (no raster conversion). Strips executable content:
/// `<script>`, `<foreignObject>`, event handlers (`on*`), and `javascript:`
/// URLs. Returns `None` if the result is empty or no longer looks like SVG.
pub fn sanitize_svg(bytes: &[u8]) -> Option<String> {
    let raw = std::str::from_utf8(bytes).ok()?;
    let mut out = String::with_capacity(raw.len());
    let lower = raw.to_ascii_lowercase();
    let bytes_raw = raw.as_bytes();
    let bytes_lower = lower.as_bytes();

    let mut i = 0;
    while i < bytes_raw.len() {
        if bytes_lower[i] == b'<' {
            // Drop entire <script>...</script> and <foreignObject>...</foreignObject>
            // blocks.
            if let Some(end) = skip_dangerous_element(bytes_lower, i) {
                i = end;
                continue;
            }

            // Copy the opening tag, but strip on* attributes and javascript: URLs.
            if let Some((tag, next)) = take_tag(bytes_raw, bytes_lower, i) {
                out.push_str(&scrub_tag_attributes(&tag));
                i = next;
                continue;
            }
        }

        out.push(bytes_raw[i] as char);
        i += 1;
    }

    let trimmed = out.trim();
    if trimmed.is_empty() || !looks_like_svg(trimmed.as_bytes()) {
        return None;
    }
    Some(trimmed.to_string())
}

fn skip_dangerous_element(lower: &[u8], start: usize) -> Option<usize> {
    const DANGEROUS: &[&[u8]] = &[b"script", b"foreignobject"];
    if lower.get(start) != Some(&b'<') {
        return None;
    }
    let after_lt = start + 1;
    let name_start = if lower.get(after_lt) == Some(&b'/') {
        return None; // closing tags handled by the open-tag skip
    } else {
        after_lt
    };

    for name in DANGEROUS {
        if lower[name_start..].starts_with(name)
            && lower
                .get(name_start + name.len())
                .map(|c| !c.is_ascii_alphanumeric() && *c != b':' && *c != b'-')
                .unwrap_or(true)
        {
            // Find matching close tag </name>
            let close = format!("</{}", std::str::from_utf8(name).unwrap());
            let close_bytes = close.as_bytes();
            let mut j = name_start + name.len();
            while j + close_bytes.len() <= lower.len() {
                if lower[j..].starts_with(close_bytes)
                    && lower
                        .get(j + close_bytes.len())
                        .map(|c| !c.is_ascii_alphanumeric())
                        .unwrap_or(true)
                {
                    // advance past '>'
                    if let Some(gt) = lower[j..].iter().position(|&c| c == b'>') {
                        return Some(j + gt + 1);
                    }
                    return Some(lower.len());
                }
                j += 1;
            }
            // No close tag — drop the rest of the document.
            return Some(lower.len());
        }
    }
    None
}

fn take_tag(raw: &[u8], lower: &[u8], start: usize) -> Option<(String, usize)> {
    if raw.get(start) != Some(&b'<') {
        return None;
    }
    let mut i = start + 1;
    let mut in_quote: Option<u8> = None;
    while i < raw.len() {
        let c = raw[i];
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
        } else if c == b'"' || c == b'\'' {
            in_quote = Some(c);
        } else if c == b'>' {
            // Prefer raw slice for output fidelity.
            let tag = String::from_utf8_lossy(&raw[start..=i]).into_owned();
            let _ = lower; // used by callers for case-insensitive decisions elsewhere
            return Some((tag, i + 1));
        }
        i += 1;
    }
    None
}

fn scrub_tag_attributes(tag: &str) -> String {
    // Fast path: no attributes that look dangerous.
    let lower = tag.to_ascii_lowercase();
    if !lower.contains("on") && !lower.contains("javascript:") {
        return tag.to_string();
    }

    // Parse: <name attrs...> or <name attrs.../>
    let bytes = tag.as_bytes();
    if bytes.first() != Some(&b'<') {
        return tag.to_string();
    }

    let mut out = String::new();
    out.push('<');

    let mut i = 1;
    // Self-closing slash / name
    if bytes.get(i) == Some(&b'/') {
        out.push('/');
        i += 1;
    }

    // Element name
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() || c == '>' || c == '/' {
            break;
        }
        out.push(c);
        i += 1;
    }

    // Attributes
    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            out.push(bytes[i] as char);
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'>' {
            out.push('>');
            break;
        }
        if bytes[i] == b'/' {
            out.push('/');
            i += 1;
            continue;
        }

        // Attribute name
        let name_start = i;
        while i < bytes.len()
            && !(bytes[i] as char).is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let name = &tag[name_start..i];
        let name_lower = name.to_ascii_lowercase();

        // Optional =value
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
                i += 1;
            }
            if let Some(&q) = bytes.get(i) {
                if q == b'"' || q == b'\'' {
                    i += 1;
                    let v_start = i;
                    while i < bytes.len() && bytes[i] != q {
                        i += 1;
                    }
                    value = tag[v_start..i].to_string();
                    if i < bytes.len() {
                        i += 1; // closing quote
                    }
                } else {
                    let v_start = i;
                    while i < bytes.len()
                        && !(bytes[i] as char).is_ascii_whitespace()
                        && bytes[i] != b'>'
                        && bytes[i] != b'/'
                    {
                        i += 1;
                    }
                    value = tag[v_start..i].to_string();
                }
            }
        }

        let drop = name_lower.starts_with("on") || value.trim().to_ascii_lowercase().starts_with("javascript:");
        if !drop {
            out.push_str(name);
            if !value.is_empty() || tag[name_start..i].contains('=') {
                out.push_str("=\"");
                out.push_str(&value.replace('"', "&quot;"));
                out.push('"');
            }
        }
    }

    if !out.ends_with('>') {
        out.push('>');
    }
    out
}

/// Deterministic SVG avatar with XML-escaped symbol text and hash-derived
/// colors.
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
    fn content_type_hints_include_common_image_formats() {
        assert_eq!(extension_for_content_type("image/png"), Some("png"));
        assert_eq!(extension_for_content_type("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for_content_type("image/webp"), Some("webp"));
        assert_eq!(extension_for_content_type("image/gif"), Some("gif"));
        assert_eq!(extension_for_content_type("image/svg+xml"), Some("svg"));
        assert_eq!(extension_for_content_type("text/html"), None);
    }

    #[test]
    fn detect_image_ext_from_magic_bytes() {
        let png = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0];
        assert_eq!(detect_image_ext(&png, "text/html"), Some("png"));

        let jpg = [0xff, 0xd8, 0xff, 0xe0, 0, 0];
        assert_eq!(detect_image_ext(&jpg, ""), Some("jpg"));

        let mut webp = b"RIFF....WEBP....".to_vec();
        webp[4] = 0;
        assert_eq!(detect_image_ext(&webp, ""), Some("webp"));

        let gif = b"GIF89a............";
        assert_eq!(detect_image_ext(gif, ""), Some("gif"));

        let svg = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        assert_eq!(detect_image_ext(svg, "image/png"), Some("svg"));
    }

    #[test]
    fn detect_image_ext_rejects_html() {
        let html = b"<!DOCTYPE html><html><body>not an image</body></html>";
        assert_eq!(detect_image_ext(html, "image/png"), None);

        let html2 = b"<html><head></head></html>";
        assert_eq!(detect_image_ext(html2, "image/svg+xml"), None);
    }

    #[test]
    fn oversized_raster_is_normalized_to_bounded_png() {
        let source = image::RgbaImage::from_fn(768, 768, |x, y| {
            let value = x.wrapping_mul(1_664_525).wrapping_add(y.wrapping_mul(1_013_904_223));
            image::Rgba([(value >> 16) as u8, (value >> 8) as u8, value as u8, 255])
        });
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        assert!(encoded.get_ref().len() as u64 > MAX_LOGO_BYTES);

        let normalized = normalize_oversized_raster(encoded.get_ref()).unwrap();
        let decoded = image::load_from_memory(&normalized).unwrap();
        assert!(normalized.len() as u64 <= MAX_LOGO_BYTES);
        assert!(decoded.width() <= MAX_RASTER_DIMENSION);
        assert!(decoded.height() <= MAX_RASTER_DIMENSION);
    }

    #[test]
    fn sanitize_svg_strips_script_and_handlers() {
        let dirty = br#"<svg xmlns="http://www.w3.org/2000/svg" onclick="alert(1)">
<script>alert(1)</script>
<a href="javascript:alert(1)"><circle r="10"/></a>
<foreignObject><body xmlns="http://www.w3.org/1999/xhtml">x</body></foreignObject>
</svg>"#;
        let clean = sanitize_svg(dirty).expect("sanitized");
        let lower = clean.to_ascii_lowercase();
        assert!(lower.contains("<svg"));
        assert!(lower.contains("<circle"));
        assert!(!lower.contains("<script"));
        assert!(!lower.contains("onclick"));
        assert!(!lower.contains("javascript:"));
        assert!(!lower.contains("foreignobject"));
    }

    #[test]
    fn sanitize_svg_rejects_non_svg() {
        assert!(sanitize_svg(b"<html></html>").is_none());
        assert!(sanitize_svg(b"").is_none());
    }
}

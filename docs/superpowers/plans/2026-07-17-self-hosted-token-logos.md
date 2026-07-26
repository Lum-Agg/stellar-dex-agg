# Self-Hosted Token Logos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Download available token logos, generate deterministic fallbacks for the rest, and serve every routed token logo from `https://api.lumagg.xyz/logos/...`.

**Architecture:** `dex-adapters` owns a filesystem-backed logo cache shared by the worker and API. The worker enriches every token metadata record with a self-hosted URL and stores files under `data/logos`; the API serves that directory with Axum/Tower HTTP. Remote images are accepted only when they are small raster images; otherwise a locally generated SVG fallback is used.

**Tech Stack:** Rust, Tokio, reqwest, SHA-256, Axum 0.8, tower-http `ServeDir`, SQLite-independent JSON metadata cache.

---

## File Structure

- Create `crates/dex-adapters/src/token_logo.rs`: safe filename generation, remote image download/validation, SVG fallback, atomic writes.
- Modify `crates/dex-adapters/src/lib.rs`: export the logo-cache module.
- Modify `crates/dex-adapters/src/token_metadata.rs`: enrich new and existing metadata with local URLs.
- Modify `crates/market-data-worker/src/worker.rs`: run logo enrichment before publishing snapshots.
- Modify `crates/api-server/src/lib.rs`: serve `data/logos` under `/logos`.
- Modify `Cargo.toml`: enable tower-http `fs`.
- Modify `packages/frontend/src/components/TokenSelector.tsx`: remove third-party runtime image fallbacks.
- Modify `deploy_server.sh`: create and preserve the shared logo directory.
- Modify `docs/integrator-guide.md` and `docs/integrator-guide.zh-CN.md`: document self-hosted logo URLs.

### Task 1: Filesystem Logo Cache

**Files:**
- Create: `crates/dex-adapters/src/token_logo.rs`
- Modify: `crates/dex-adapters/src/lib.rs`
- Test: `crates/dex-adapters/src/token_logo.rs`

- [ ] **Step 1: Write failing unit tests**

Test that:

```rust
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
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p dex-adapters token_logo
```

Expected: FAIL because `TokenLogoCache` and helpers do not exist.

- [ ] **Step 3: Implement the minimal cache**

Implement:

```rust
pub struct TokenLogoCache {
    directory: PathBuf,
    base_url: String,
    client: reqwest::Client,
}

impl TokenLogoCache {
    pub fn from_env() -> Self;
    pub fn new(directory: impl Into<PathBuf>, base_url: impl Into<String>) -> Self;
    pub async fn ensure_logo(
        &self,
        token_id: &str,
        symbol: &str,
        remote_url: Option<&str>,
    ) -> anyhow::Result<String>;
}
```

Requirements:

- `TOKEN_LOGO_DIR` defaults to `data/logos`.
- `TOKEN_LOGO_BASE_URL` defaults to `https://api.lumagg.xyz/logos`.
- Filename is SHA-256 of `token_id`, so token-controlled text never enters a path.
- Accept only successful `image/png`, `image/jpeg`, or `image/webp` responses no larger than 1 MiB.
- Use a 10-second request timeout.
- Write to `*.tmp`, then atomically rename.
- If download fails or no source exists, write a generated SVG containing escaped symbol text and a color derived from the token hash.
- Existing files are reused without another download.
- Return `<base_url>/<filename>`.

- [ ] **Step 4: Export the module**

Add to `crates/dex-adapters/src/lib.rs`:

```rust
pub mod token_logo;
```

- [ ] **Step 5: Run focused tests**

```bash
cargo test -p dex-adapters token_logo
```

Expected: all `token_logo` tests PASS.

### Task 2: Enrich All Token Metadata

**Files:**
- Modify: `crates/dex-adapters/src/token_metadata.rs`
- Test: `crates/dex-adapters/src/token_metadata.rs`

- [ ] **Step 1: Write a failing fallback enrichment test**

Create a temporary logo directory, insert metadata with `logo: None`, run enrichment, and assert:

```rust
assert!(metadata.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
```

- [ ] **Step 2: Run the test and verify failure**

```bash
cargo test -p dex-adapters token_metadata::tests::enriches_missing_logo_with_self_hosted_fallback
```

Expected: FAIL because existing metadata is never re-enriched.

- [ ] **Step 3: Integrate `TokenLogoCache`**

Add a cache field:

```rust
pub struct TokenMetadataStore {
    cache: Arc<RwLock<HashMap<String, TokenMetadata>>>,
    logo_cache: Arc<TokenLogoCache>,
}
```

Add a testable constructor:

```rust
pub fn with_logo_cache(logo_cache: TokenLogoCache) -> Self;
```

Update `resolve_unknown` so each newly resolved token calls `ensure_logo` before insertion. Add:

```rust
pub async fn ensure_self_hosted_logos(&self) -> usize;
```

This method must:

- clone metadata entries without holding the write lock across HTTP or filesystem awaits;
- treat the current external `logo` as the download source;
- skip redownload when `logo` already starts with `TOKEN_LOGO_BASE_URL`;
- generate a fallback for entries with no usable remote source;
- write updated URLs back to the map;
- persist `data/token_metadata.json` after changes;
- return the number of metadata records with a self-hosted logo.

- [ ] **Step 4: Ensure existing cached tokens are backfilled**

In `resolve_unknown`, call `ensure_self_hosted_logos()` even when `unknown.is_empty()`. This is required because production already has 249 cached metadata entries.

- [ ] **Step 5: Run metadata tests**

```bash
cargo test -p dex-adapters token_metadata
```

Expected: all metadata tests PASS.

### Task 3: Publish Self-Hosted URLs in Market Snapshots

**Files:**
- Modify: `crates/market-data-worker/src/worker.rs`
- Test: existing worker metadata tests

- [ ] **Step 1: Update enrichment ordering**

In `spawn_token_metadata_enrichment`, ensure this order:

```rust
token_metadata.resolve_unknown(token_addresses.clone()).await;
token_metadata.ensure_self_hosted_logos().await;
let metadata = token_metadata.get_all().await;
```

This guarantees the published Redis snapshot contains local URLs rather than stale third-party URLs.

- [ ] **Step 2: Run worker tests**

```bash
cargo test -p market-data-worker --lib
```

Expected: PASS.

### Task 4: Serve Logos from the API

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/api-server/src/lib.rs`
- Test: `crates/api-server/src/lib.rs`

- [ ] **Step 1: Enable static-file support**

Change the workspace dependency to:

```toml
tower-http = { version = "0.6", features = ["cors", "trace", "fs"] }
```

- [ ] **Step 2: Add a testable router constructor**

Extract router creation:

```rust
fn build_router(app_state: AppState, rate_limit: RateLimitState, logo_dir: PathBuf) -> Router {
    let api = Router::new()
        // existing API routes
        .layer(middleware::from_fn_with_state(
            rate_limit,
            rate_limit::rate_limit_middleware,
        ));

    Router::new()
        .merge(api)
        .nest_service("/logos", ServeDir::new(logo_dir))
        .layer(CorsLayer::permissive())
        .with_state(app_state)
}
```

Static logo requests must not consume API rate-limit quota.

- [ ] **Step 3: Configure the directory**

In `run_server`:

```rust
let logo_dir = std::env::var("TOKEN_LOGO_DIR")
    .unwrap_or_else(|_| "data/logos".to_string());
```

Create the directory before binding and pass it to `build_router`.

- [ ] **Step 4: Add static serving test**

Write a temporary `sample.svg`, request `/logos/sample.svg` with `tower::ServiceExt`, and assert:

```rust
assert_eq!(response.status(), StatusCode::OK);
assert_eq!(
    response.headers().get(CONTENT_TYPE).unwrap(),
    "image/svg+xml"
);
```

- [ ] **Step 5: Run API tests**

```bash
cargo test -p api-server --lib
```

Expected: PASS.

### Task 5: Remove Third-Party Frontend Fallbacks

**Files:**
- Modify: `packages/frontend/src/components/TokenSelector.tsx`

- [ ] **Step 1: Remove hard-coded stellar.expert logo URLs**

Delete `WELL_KNOWN_CONTRACT_LOGOS`. Priority tokens should receive logos from `/api/v1/tokens`; before the API response, render the existing deterministic initial-letter avatar.

- [ ] **Step 2: Verify frontend**

```bash
cd packages/frontend
npm run build
```

Expected: static build PASS, with no `stellar.expert/explorer/public/asset` reference in `TokenSelector.tsx`.

### Task 6: Deployment Persistence and Documentation

**Files:**
- Modify: `deploy_server.sh`
- Modify: `docs/integrator-guide.md`
- Modify: `docs/integrator-guide.zh-CN.md`

- [ ] **Step 1: Preserve runtime data**

Ensure deployment creates but never deletes:

```bash
mkdir -p "${REMOTE_APP_DIR}/data/logos"
```

The source rsync targets `${REMOTE_SRC}`, so it must not overwrite `${REMOTE_APP_DIR}/data`.

- [ ] **Step 2: Document the API contract**

Document that `/api/v1/tokens[].logo` is either empty only during startup enrichment or an absolute self-hosted URL under:

```text
https://api.lumagg.xyz/logos/
```

- [ ] **Step 3: Run full verification**

```bash
cargo fmt --all -- --check
cargo test -p dex-adapters
cargo test -p market-data-worker --lib
cargo test -p api-server --lib
cargo check --workspace
```

Expected: all commands PASS without warnings.

### Task 7: Deploy and Verify Tranche 1 Acceptance

**Files:**
- No source changes

- [ ] **Step 1: Deploy worker and API**

```bash
./deploy_server.sh all
```

- [ ] **Step 2: Wait for metadata republish**

```bash
ssh root@88.198.16.144 \
  'journalctl -u lumagg-worker --since "10 minutes ago" --no-pager | grep "token metadata enrichment"'
```

Expected: snapshot republished after metadata enrichment.

- [ ] **Step 3: Verify logo coverage and ownership**

```bash
curl -fsS https://api.lumagg.xyz/api/v1/tokens | jq '
  (.data // .tokens // .) as $tokens |
  {
    total: ($tokens | length),
    with_logo: ([$tokens[] | select(.logo != "")] | length),
    self_hosted: ([$tokens[] | select(.logo | startswith("https://api.lumagg.xyz/logos/"))] | length)
  }'
```

Expected:

- `with_logo >= 50`;
- `self_hosted == with_logo`;
- after enrichment completes, preferably `self_hosted == total`.

- [ ] **Step 4: Verify representative files**

```bash
curl -fsSI "$(curl -fsS https://api.lumagg.xyz/api/v1/tokens |
  jq -r '(.data // .tokens // .)[0].logo')"
```

Expected: HTTP 200 and `Content-Type: image/*`.

- [ ] **Step 5: Confirm the frontend**

Open `https://lumagg.xyz`, search several tokens, and confirm real images or deterministic self-hosted fallback avatars render without third-party image requests.

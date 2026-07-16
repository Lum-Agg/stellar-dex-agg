# Unified Embedded Swap-API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One quote/refresh codebase with pluggable stores so self-hosters run a single binary (`embedded` = in-process worker + memory) while production keeps Redis cluster mode—without forking business logic.

**Architecture:** Extract `PoolStateStore` (and reuse existing `SnapshotStore`) behind traits. Redis and Memory backends implement the same APIs. Worker publish + API hydrate only talk to `Arc<dyn …>`. `LUMAGG_MODE=embedded|cluster` selects wiring, not alternate quote paths.

**Tech Stack:** Rust workspace (`market-snapshot`, `market-data-worker` as lib, `api-server`), `async-trait`, Tokio, existing Redis clients.

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/market-snapshot/src/pool_state_store.rs` | `PoolStateStore` trait; `RedisPoolStateStore`; new `MemoryPoolStateStore` |
| `crates/market-snapshot/src/store.rs` | Add `MemorySnapshotStore` + `SnapshotStoreBackend::Memory` |
| `crates/market-data-worker/src/lib.rs` | Export `worker::run` / `WorkerConfig` for embedding |
| `crates/market-data-worker/src/worker.rs` | Inject `Arc<dyn SnapshotStore>` + `Arc<dyn PoolStateStore>` |
| `crates/api-server/src/pool_hydrate.rs` | Hydrate via trait object |
| `crates/api-server/src/state.rs` / `config.rs` | Mode wiring; spawn worker in embedded |
| Docs: short `README` / deploy note for `LUMAGG_MODE` | Self-host one-binary instructions |

---

### Task 1: `PoolStateStore` trait + `MemoryPoolStateStore`

**Files:**
- Modify: `crates/market-snapshot/src/pool_state_store.rs`
- Test: unit tests in same file

- [ ] **Step 1:** Define `#[async_trait] trait PoolStateStore` with: `publish_pool_state`, `set_xyk_batch`, `set_clmm_batch`, `set_aquarius_batch`, `set_comet_batch`, `fetch_xyk`, `fetch_clmm`, `fetch_aquarius`, `fetch_comet` (mirror current Redis methods used by worker/API).
- [ ] **Step 2:** `impl PoolStateStore for RedisPoolStateStore` (thin wrappers calling existing methods).
- [ ] **Step 3:** Implement `MemoryPoolStateStore` with `tokio::sync::RwLock<HashMap<…>>` (or four maps); no TTL required for v1 (or optional ignore).
- [ ] **Step 4:** Unit test: set_xyk_batch → fetch_xyk round-trip on Memory.
- [ ] **Step 5:** `cargo test -p market-snapshot`

---

### Task 2: `MemorySnapshotStore`

**Files:**
- Modify: `crates/market-snapshot/src/store.rs`

- [ ] **Step 1:** `MemorySnapshotStore { current: RwLock<Option<MarketSnapshot>>, version_tx: watch::Sender<Option<String>> }`.
- [ ] **Step 2:** `impl SnapshotStore`; on publish, update + send version.
- [ ] **Step 3:** Expose `subscribe_versions() -> watch::Receiver<Option<String>>` for API reload (replaces Redis pub/sub in embedded).
- [ ] **Step 4:** Extend `SnapshotStoreBackend` with `Memory` and `build_snapshot_store` branch.
- [ ] **Step 5:** `cargo test -p market-snapshot`

---

### Task 3: Worker accepts injected stores

**Files:**
- Create: `crates/market-data-worker/src/lib.rs`
- Modify: `worker.rs`, `fetch_pipeline.rs`, `touched_refresh.rs`, `monitor.rs`, `main.rs`, `Cargo.toml`

- [ ] **Step 1:** Add `lib.rs` re-exporting `worker::{run, WorkerConfig}` (and needed types).
- [ ] **Step 2:** Change `WorkerConfig` / `run` to take `snapshot_store: Arc<dyn SnapshotStore>`, `pool_store: Option<Arc<dyn PoolStateStore>>` (build from env in `main` for cluster; callers can pass memory).
- [ ] **Step 3:** Replace `RedisPoolStateStore` concrete types with `dyn PoolStateStore` at publish/fetch call sites.
- [ ] **Step 4:** `cargo build -p market-data-worker`

---

### Task 4: API hydrate + state use trait

**Files:**
- Modify: `pool_hydrate.rs`, `state.rs`, `verify_split_quote.rs`

- [ ] **Step 1:** `hydrate_paths(..., store: &dyn PoolStateStore, ...)`.
- [ ] **Step 2:** `AppState.pool_state_store: Option<Arc<dyn PoolStateStore>>`.
- [ ] **Step 3:** Keep Redis construction for cluster; compile + existing tests.

---

### Task 5: `LUMAGG_MODE=embedded` in api-server

**Files:**
- Modify: `crates/api-server/src/config.rs`, `lib.rs` / server startup, `Cargo.toml` (depend on `market-data-worker`)

- [ ] **Step 1:** Parse `LUMAGG_MODE` (`embedded` | `cluster`, default `cluster` for prod safety—or `embedded` if no Redis URL).
- [ ] **Step 2:** Embedded: create shared Memory stores, `tokio::spawn(worker::run(...))`, load/reload snapshot via memory watch, skip Redis requirement.
- [ ] **Step 3:** Cluster: unchanged Redis worker + API split.
- [ ] **Step 4:** Smoke: `cargo build -p api-server`; document env in README snippet.

---

### Task 6: Docs + sanity

- [ ] Short self-host section: one binary, `RPC_URL`, `LUMAGG_MODE=embedded`, listen port.
- [ ] Note production remains Redis + separate `market-data-worker`.

---

## Out of scope (YAGNI)

- Dual quote engines / legacy non-snapshot refresh fork
- Horizontal multi-API without Redis
- Changing arb to embed quote (can call HTTP swap-api)

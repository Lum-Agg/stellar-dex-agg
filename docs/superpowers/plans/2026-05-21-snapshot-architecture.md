# Snapshot Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple market-data discovery/refresh from `api-server` so multiple API instances can serve `quote` and `build_tx` from a shared snapshot instead of in-process adapters.

**Architecture:** Introduce a shared `market-snapshot` crate for serializable market state, a `market-data-worker` binary that publishes file-backed snapshots, and an `api-server` startup path that hydrates a fresh `QuoteEngine` from snapshot data and hot-reloads it by swapping the engine pointer. Keep route computation in local memory and move only state production out of the API process.

**Tech Stack:** Rust workspace crates, Axum, Tokio, Serde JSON, existing `router-engine` and `dex-adapters`.

---

### Task 1: Shared snapshot crate

**Files:**
- Create: `crates/market-snapshot/Cargo.toml`
- Create: `crates/market-snapshot/src/lib.rs`
- Modify: `Cargo.toml`
- Test: `cargo test -p market-snapshot`

- [ ] **Step 1: Add the crate to the workspace with serde/anyhow dependencies**
- [ ] **Step 2: Define `MarketSnapshot`, `SnapshotMeta`, `SourceSnapshot`, `TradingPairSnapshot`, and file-store metadata helpers**
- [ ] **Step 3: Add a round-trip serialization test for a representative snapshot**
- [ ] **Step 4: Run `cargo test -p market-snapshot` and make sure it passes**

### Task 2: Snapshot-backed engine builder in `api-server`

**Files:**
- Create: `crates/api-server/src/snapshot_loader.rs`
- Modify: `crates/api-server/src/state.rs`
- Modify: `crates/api-server/src/config.rs`
- Modify: `crates/api-server/Cargo.toml`
- Test: `cargo test -p api-server snapshot_loader`

- [ ] **Step 1: Add failing tests for building a `QuoteEngine` from snapshot data and for loading the current snapshot from disk**
- [ ] **Step 2: Implement file-backed snapshot loading and `build_engine_from_snapshot()` by reusing `QuoteEngine::update_pairs_from_cache()`**
- [ ] **Step 3: Refactor `AppState` to hold a replaceable `Arc<QuoteEngine>` behind `RwLock` and add a polling reloader task**
- [ ] **Step 4: Gate the old adapter/discovery startup behind config so snapshot mode can run without in-process refresh loops**
- [ ] **Step 5: Run focused `api-server` tests to verify snapshot startup works**

### Task 3: Market data worker

**Files:**
- Create: `crates/market-data-worker/Cargo.toml`
- Create: `crates/market-data-worker/src/main.rs`
- Create: `crates/market-data-worker/src/worker.rs`
- Modify: `Cargo.toml`
- Test: `cargo test -p market-data-worker`

- [ ] **Step 1: Add the worker crate to the workspace and wire shared dependencies**
- [ ] **Step 2: Move discovery/refresh helper logic out of `api-server::state` into the worker while preserving current behavior**
- [ ] **Step 3: Publish `MarketSnapshot` to a file path (`data/snapshots/current.json` plus metadata) with atomic replace semantics**
- [ ] **Step 4: Add at least one test covering snapshot publication from sanitized trading pairs**
- [ ] **Step 5: Run `cargo test -p market-data-worker` and make sure it passes**

### Task 4: End-to-end snapshot mode verification

**Files:**
- Modify: `README.md`
- Modify: `PROJECT_STATUS.md`
- Test: `cargo test -p market-snapshot && cargo test -p api-server && cargo test -p market-data-worker`

- [ ] **Step 1: Document how to run the worker and API in snapshot mode**
- [ ] **Step 2: Generate a snapshot locally, boot `api-server` from it, and confirm `/api/v1/quote` and `/api/v1/build_tx` still work**
- [ ] **Step 3: Run the focused test suite and record any known gaps (Redis not yet implemented, worker singleton assumption)**

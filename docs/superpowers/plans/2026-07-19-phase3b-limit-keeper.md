# Phase 3b — Limit Keeper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Escrow emits create/cancel/expire events; new `limit-keeper` crate polls getEvents, quotes, and submits `fill` when limits are met.

**Architecture:** Small contract event patch + standalone keeper binary. Reuse `dex_adapters` RPC events + arb-style quote HTTP and simulate/sign/submit patterns without coupling to vault round-trip.

**Tech Stack:** Rust workspace, soroban RPC (`dex_adapters::rpc`), `reqwest` quote API, `soroban-client` / `stellar-baselib` sign+submit (mirror `crates/arbitrage`).

**Spec:** `docs/superpowers/specs/2026-07-19-phase3b-limit-keeper-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `contracts/order-escrow/src/lib.rs` | Emit `order_created` / `order_cancelled` / `order_expired` |
| `crates/limit-keeper/` | New crate + bin `limit-keeper` |
| Root `Cargo.toml` | workspace member |
| `docs/...` | Short operator note (optional in Task 6) |

---

### Task 1: Escrow lifecycle events

**Files:** `contracts/order-escrow/src/lib.rs`

- [ ] **Step 1: Tests** assert create/cancel/reclaim publish expected topic symbols (use `env.events().all()` pattern from soroban testutils if available in this SDK version; else assert via successful call + documented event payload shape and a unit helper).

Minimal approach matching existing style: after `create_limit`, inspect `env.events().all()` for topic containing `order_created`.

- [ ] **Step 2: Implement publishes**

In `create_limit` before return:

```rust
env.events().publish(
    (Symbol::new(&env, "order_created"), order_id),
    (
        order.owner.clone(),
        order.token_in.clone(),
        order.token_out.clone(),
        amount_in,
        limit_out_per_in_e7,
        expires_ledger,
    ),
);
```

In `cancel` / `reclaim_expired` similarly with `order_cancelled` / `order_expired`.

- [ ] **Step 3:** `cargo test -p order-escrow-contract`

- [ ] **Step 4: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(escrow): emit create/cancel/expire events for keepers

Enable getEvents-based open-order discovery for limit-keeper.
EOF
)"
```

---

### Task 2: Scaffold `limit-keeper` crate + limit math

**Files:**
- Create `crates/limit-keeper/Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/limit.rs`, `src/config.rs`
- Add workspace member

- [ ] **Step 1: Failing tests in `limit.rs`**

```rust
#[test]
fn required_min_out_scales() {
    assert_eq!(required_min_out(5_000_000, 20_000_000), 10_000_000);
}
```

- [ ] **Step 2: Implement + config stub reading env names from spec**

- [ ] **Step 3:** `cargo test -p limit-keeper`

- [ ] **Step 4: Commit** `feat(keeper): scaffold limit-keeper with limit math`

---

### Task 3: Event parsing + order book

**Files:** `events.rs`, `book.rs`

- [ ] Parse `ContractEvent` topics for `order_created` / `order_filled` / `order_cancelled` / `order_expired` (mirror `analytics-indexer/src/events.rs` topic extraction).
- [ ] `OpenOrderBook`: insert on created; update remaining on filled; remove on cancelled/expired/filled remaining 0.
- [ ] Unit tests with synthetic `ScVal` topic/data fixtures (construct minimal events if hard — test parser functions on decoded fields).

- [ ] Commit `feat(keeper): parse escrow events into open order book`

---

### Task 4: Quote + fillability gate

**Files:** `quote.rs`, reuse patterns from `arbitrage/src/quote_client.rs`

- [ ] `fetch_quote(token_in, token_out, amount_in) -> Quote`
- [ ] `is_fillable(order, quote) -> bool` using `required_min_out` vs `expected_output`
- [ ] Unit test fillable / not fillable with fake quote numbers
- [ ] Commit `feat(keeper): quote gate for executable limits`

---

### Task 5: Build fill tx + dry-run loop

**Files:** `execute.rs`, `main.rs`, cursor file IO

- [ ] Build Soroban invoke for `fill(order_id, amount_in, sub_routes, min_amount_out)`  
  Map quote `sub_routes` to `lumagg`/XDR `SubRoute` the same way arb maps quotes to swap steps — **read escrow fill ABI and shared-types**. Prefer importing types from a small helper; if XDR build is heavy, adapt arb `invoke`/`prepare` modules by copying only what's needed (avoid vault).
- [ ] Simulate; on success and `!dry_run`, sign with `KEEPER_SECRET` and submit.
- [ ] Main loop: load cursor → getEvents → update book → try fills → save cursor.
- [ ] `KEEPER_DRY_RUN=1` skips submit.
- [ ] Commit `feat(keeper): poll events and submit escrow fills`

**Note:** If full simulate/submit in one task is too large, split: (5a) build unsigned XDR + dry-run log; (5b) sign/submit. Prefer one commit if feasible.

---

### Task 6: Operator docs + verify

- [ ] Short `crates/limit-keeper/README.md` with env table and dry-run example
- [ ] `cargo test -p limit-keeper` && `cargo test -p order-escrow-contract`
- [ ] Commit `docs(keeper): operator README for limit-keeper`

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| Escrow lifecycle events | 1 |
| New limit-keeper crate | 2 |
| getEvents discovery | 3, 5 |
| Quote + limit gate | 4 |
| fill submit / dry-run | 5 |
| No DCA/UI/API | enforced |

## Out of scope

- DCA, REST orders API, frontend, incentives, permissionless competition tooling

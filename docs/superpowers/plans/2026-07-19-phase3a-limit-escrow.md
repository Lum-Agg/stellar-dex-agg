# Phase 3a — Limit Order Escrow Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Soroban `order-escrow` contract that can create/cancel/expire **limit** orders and permissionlessly `fill` them by invoking existing `aggregator.swap` — proven with unit tests (mock pool like vault tests). **No DCA, no keeper, no UI in this plan.**

**Architecture:** Escrow locks `token_in` on create. Filler calls `fill` with `sub_routes` + sizes; escrow verifies limit rate and expiry, then calls aggregator with `user = escrow` (preferred) after an **auth spike**. Fallback if auth fails: vault-like temporary caller path is **out of scope** — instead add a minimal `swap_from`/document invoker-auth fix on aggregator only if spike proves `user=escrow` impossible.

**Tech Stack:** `soroban-sdk` 22 (match vault/aggregator), `lumagg-contract-types`, workspace member `contracts/order-escrow`, TDD with `soroban-sdk` testutils.

**Spec:** `docs/superpowers/specs/2026-07-19-phase3-limit-dca-architecture.md` (default escrow; Limit only for 3a).

---

## Locked product rules (3a)

| Rule | Value |
|------|--------|
| Order type | Limit only |
| Limit encoding | `limit_out_per_in_e7`: min `token_out` stroops required per `10_000_000` stroops of `token_in` |
| Fill check | For fill size `x`, require `min_amount_out >= (x * limit_out_per_in_e7) / 10_000_000` (i128 math, floor) |
| Partial fills | Yes — reduce `amount_in_remaining` |
| Cancel | Owner refunds remaining `token_in` |
| Expire | `expires_ledger`; `reclaim_expired` refunds remaining (permissionless) |
| Fill auth | Anyone |
| Aggregator | Existing `swap(user, token_in, token_out, sub_routes, min_amount_out)` |

---

## File map

| File | Responsibility |
|------|----------------|
| `contracts/order-escrow/Cargo.toml` | Package deps |
| `contracts/order-escrow/src/lib.rs` | Contract + tests |
| `Cargo.toml` (workspace) | Add member |
| `contracts/order-escrow/README.md` | Brief ops / ABI notes |
| Optionally `contracts/aggregator/...` | **Only if** Task 1 spike requires a tiny auth helper |

---

### Task 1: Aggregator auth spike (decide `user=escrow`)

**Files:**
- Create: `contracts/order-escrow/` skeleton + spike test, **or** temporary test under `contracts/vault` pattern
- Prefer new crate immediately so spike lives as `tests` module next to real code

- [ ] **Step 1: Scaffold crate**

`contracts/order-escrow/Cargo.toml`:

```toml
[package]
name = "order-escrow-contract"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
soroban-sdk = "22.0.0"
lumagg-contract-types = { path = "../shared-types" }

[dev-dependencies]
soroban-sdk = { version = "22.0.0", features = ["testutils"] }
aggregator-contract = { path = "../aggregator", default-features = false }
lumagg-contract-types = { path = "../shared-types" }
```

Add to workspace `Cargo.toml` members: `"contracts/order-escrow"`.

Minimal `lib.rs` with `OrderEscrowContract` stub `initialize(admin)` so it registers.

- [ ] **Step 2: Write spike test** (may live in `lib.rs` `#[cfg(test)]`)

Copy mock Aquarius 1:1 pool + SAC setup from `contracts/vault/src/lib.rs` tests.

Spike scenario:
1. Register aggregator + initialize
2. Register escrow stub that holds token_in balance
3. Mint token_in to escrow address
4. Escrow (test helper contract method `spike_swap_as_self`) calls `aggregator.swap(user=escrow, …)` with mock route
5. Assert token_out credited to escrow (or user if forward)

```rust
#[test]
fn spike_escrow_can_be_aggregator_user() {
    // If this panics on require_auth / transfer, document failure mode.
}
```

- [ ] **Step 3: Run spike**

```bash
cargo test -p order-escrow-contract spike_escrow_can_be_aggregator_user -- --nocapture
```

- [ ] **Step 4: Record outcome in crate README + commit**

**If PASS:** Document “fill uses `user = escrow`; after swap, escrow transfers `token_out` to owner”. Proceed Task 2.

**If FAIL:** Implement **minimal** aggregator change in same task:

Option preferred if needed — add:

```rust
/// Like `swap`, but `payer` funds are pulled with payer auth already established
/// by a calling contract that is `payer`. Same body as swap after auth.
pub fn swap_for(env, payer: Address, …) 
```

Actually if `user.require_auth()` fails when caller is escrow and user=escrow, try authorizing with `env.authorize_as_current_contract` before cross-call from escrow. Prefer fixing **from escrow side** first:

```rust
// In escrow fill, before agg.swap:
env.authorize_as_current_contract(vec![&env, /* auth entries for token.transfer */]);
```

Only modify aggregator if escrow-side auth cannot work. Capture decision in README.

Commit:

```bash
git commit -m "$(cat <<'EOF'
feat(escrow): scaffold order-escrow and prove aggregator auth path

Spike whether escrow can act as aggregator.swap user before Limit ABI.
EOF
)"
```

---

### Task 2: Limit math + storage types (unit tests, no fill yet)

**Files:** `contracts/order-escrow/src/lib.rs`

- [ ] **Step 1: Failing tests for pure helpers**

```rust
#[test]
fn min_out_scales_with_fill_size() {
    // limit_out_per_in_e7 = 2e7 means 2 out per 1 in
    // x=5e6 → min = 1e7
    assert_eq!(required_min_out(5_000_000, 20_000_000), 10_000_000);
}

#[test]
fn min_out_zero_and_overflow_guards() {
    assert_eq!(required_min_out(0, 20_000_000), 0);
}
```

Implement:

```rust
fn required_min_out(amount_in: i128, limit_out_per_in_e7: i128) -> i128 {
    assert!(amount_in >= 0);
    assert!(limit_out_per_in_e7 > 0);
    amount_in
        .checked_mul(limit_out_per_in_e7)
        .expect("mul")
        / 10_000_000
}
```

- [ ] **Step 2: Define storage**

```rust
#[contracttype]
pub struct LimitOrder {
    pub owner: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in_remaining: i128,
    pub limit_out_per_in_e7: i128,
    pub expires_ledger: u32,
    pub status: OrderStatus, // Open=0, Filled=1, Cancelled=2, Expired=3 — or use enum
}

#[contracttype]
pub enum DataKey {
    Admin,
    Aggregator,
    NextOrderId,
    Order(u64),
}
```

- [ ] **Step 3: `initialize(admin, aggregator)`** — admin auth set once

- [ ] **Step 4: Commit** `feat(escrow): limit rate math and order storage types`

---

### Task 3: `create_limit` + `cancel`

- [ ] **Step 1: Tests**

```rust
#[test]
fn create_limit_pulls_token_in() { … }

#[test]
fn owner_can_cancel_and_refund() { … }

#[test]
fn non_owner_cannot_cancel() { … }
```

- [ ] **Step 2: Implement**

```rust
pub fn create_limit(
    env: Env,
    owner: Address,
    token_in: Address,
    token_out: Address,
    amount_in: i128,
    limit_out_per_in_e7: i128,
    expires_ledger: u32,
) -> u64 {
    owner.require_auth();
    assert!(amount_in > 0);
    assert!(limit_out_per_in_e7 > 0);
    assert!(token_in != token_out);
    assert!(expires_ledger > env.ledger().sequence());
    // transfer owner → escrow
    // store order, bump next id, emit event
}

pub fn cancel(env: Env, order_id: u64) {
    // owner.require_auth(); refund remaining; status=Cancelled
}
```

- [ ] **Step 3: `cargo test -p order-escrow-contract`**

- [ ] **Step 4: Commit** `feat(escrow): create_limit and cancel with refunds`

---

### Task 4: `fill` (permissionless) + partial fill

- [ ] **Step 1: Tests with mock pool (vault style)**

```rust
#[test]
fn fill_executes_when_limit_met() { … }

#[test]
fn fill_rejects_when_min_out_below_limit() { … }

#[test]
fn fill_rejects_expired() { … }

#[test]
fn partial_fill_reduces_remaining() { … }

#[test]
fn anyone_can_fill() { /* filler != owner */ }
```

- [ ] **Step 2: Implement `fill`**

```rust
pub fn fill(
    env: Env,
    filler: Address, // optional: unused except events; or omit
    order_id: u64,
    amount_in: i128,
    sub_routes: Vec<SubRoute>,
    min_amount_out: i128,
) -> i128 {
    // no owner auth
    // load Open order; check !expired; amount_in > 0; amount_in <= remaining
    // assert sub_routes sum == amount_in
    // assert min_amount_out >= required_min_out(amount_in, order.limit_out_per_in_e7)
    // call aggregator.swap(user=escrow_addr, token_in, token_out, sub_routes, min_amount_out)
    // transfer token_out from escrow → owner (full amount returned by swap)
    // remaining -= amount_in; if 0 status=Filled
    // emit OrderFilled
}
```

Use `authorize_as_current_contract` as required by Task 1 outcome.

- [ ] **Step 3: Tests pass**

- [ ] **Step 4: Commit** `feat(escrow): permissionless fill via aggregator.swap`

---

### Task 5: `reclaim_expired`

- [ ] **Step 1: Test** expire by bumping ledger sequence in testutils; reclaim refunds; second reclaim fails

- [ ] **Step 2: Implement**

```rust
pub fn reclaim_expired(env: Env, order_id: u64) {
    // status Open && ledger > expires → refund remaining to owner, status Expired
}
```

- [ ] **Step 3: Commit** `feat(escrow): reclaim_expired refunds open residual`

---

### Task 6: README + workspace polish

- [ ] Document ABI, limit math example, auth decision from Task 1, explicit **DCA out of scope**
- [ ] Ensure `cargo test -p order-escrow-contract` all green
- [ ] Commit `docs(escrow): README for limit order escrow ABI`

---

### Task 7: Verification checklist

- [ ] All escrow tests pass
- [ ] Aggregator crate still passes its tests (`cargo test -p aggregator-contract`)
- [ ] No keeper/API/UI files added
- [ ] Workspace builds

---

## Spec coverage

| Research item | Task |
|---------------|------|
| Escrow default | all |
| Limit rate + partial fill | 2–4 |
| Permissionless fill | 4 |
| Cancel / expire | 3, 5 |
| Aggregator auth spike | 1 |
| DCA deferred | locked in this plan |
| Keeper / UI / incentives | not in 3a |

## Out of scope (do not implement)

- DCA order type
- limit-keeper binary
- Order REST API / indexer
- Frontend Limit tab
- Fill tips / rebates
- Mainnet deploy scripts (optional follow-up after tests; not required to close 3a)

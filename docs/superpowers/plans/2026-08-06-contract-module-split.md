# Contract Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split Soroban contracts from single-file `lib.rs` into focused modules without changing external behavior, storage, events, or auth semantics.

**Architecture:** Behavior-preserving refactor. `lib.rs` becomes a thin `#[contractimpl]` facade; logic moves into `storage` / `auth` / domain modules. Validate after each contract with existing unit tests. Start with `vault`, then `aggregator`, leave `order-escrow` for a follow-up once the pattern is proven.

**Tech Stack:** Rust, Soroban SDK 22, `cargo test -p vault-contract` / `cargo test -p aggregator-contract`

**Spec:** `docs/superpowers/specs/2026-08-06-contract-module-split-design.md`

---

## File map

### Phase 1 — `contracts/vault`

| File | Role |
|------|------|
| `contracts/vault/src/lib.rs` | `mod` declarations + thin `#[contractimpl]` facade |
| `contracts/vault/src/types.rs` | Aggregator `#[contractclient]` trait |
| `contracts/vault/src/storage.rs` | `DataKey` + admin / caller storage helpers |
| `contracts/vault/src/auth.rs` | `require_admin` / `require_caller` |
| `contracts/vault/src/admin.rs` | initialize, upgrade, caller admin, withdraw |
| `contracts/vault/src/execute.rs` | deposit + execute_round_trip |
| `contracts/vault/src/tests.rs` | Existing tests moved unchanged |

### Phase 2 — `contracts/aggregator` (after vault green)

| File | Role |
|------|------|
| `contracts/aggregator/src/lib.rs` | Thin facade |
| `contracts/aggregator/src/storage.rs` | `DataKey` + admin helpers |
| `contracts/aggregator/src/auth.rs` | admin auth |
| `contracts/aggregator/src/math.rs` | fee / amount_out / scale helpers |
| `contracts/aggregator/src/validate.rs` | `validate_sub_routes` |
| `contracts/aggregator/src/events.rs` | swap / rt event emit helpers |
| `contracts/aggregator/src/invoke.rs` | `execute_step*` / DEX invoke bodies |
| `contracts/aggregator/src/admin.rs` | initialize / upgrade / admin |
| `contracts/aggregator/src/swap.rs` | `swap` + `execute_sub_routes` / `execute_path` |
| `contracts/aggregator/src/round_trip.rs` | `round_trip_swap` |
| `contracts/aggregator/src/tests.rs` | Existing `mod test` body moved (keep one file first) |

### Out of scope for this plan

- `contracts/order-escrow` (follow-up after aggregator)
- Any storage / event / signature / WASM upgrade of deployed contracts
- Frontend / indexer changes

---

### Task 1: Baseline — prove current vault tests pass

**Files:**
- Read only: `contracts/vault/src/lib.rs`

- [ ] **Step 1: Run vault tests**

```bash
cargo test -p vault-contract
```

Expected: all tests PASS (currently `execute_round_trip_returns_funds_to_vault`, `non_caller_cannot_execute`).

- [ ] **Step 2: Note any failures before refactor**

If anything fails, stop and fix baseline first. Do not start splitting on a red tree.

- [ ] **Step 3: Commit nothing**

Baseline only. No code changes in this task.

---

### Task 2: Extract vault `types.rs` + `storage.rs`

**Files:**
- Create: `contracts/vault/src/types.rs`
- Create: `contracts/vault/src/storage.rs`
- Modify: `contracts/vault/src/lib.rs`

- [ ] **Step 1: Create `types.rs` with the aggregator client trait**

Move this from `lib.rs` unchanged:

```rust
use {
    lumagg_contract_types::SubRoute,
    soroban_sdk::{contractclient, Address, Env, Vec},
};

#[contractclient(name = "AggregatorContractClient")]
pub trait AggregatorContract {
    fn round_trip_swap(
        env: Env,
        user: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128;
}
```

- [ ] **Step 2: Create `storage.rs`**

```rust
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Caller(Address),
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("Not initialized")
}

pub fn set_caller(env: &Env, caller: &Address, allowed: bool) {
    if allowed {
        env.storage().persistent().set(&DataKey::Caller(caller.clone()), &true);
    } else {
        env.storage().persistent().remove(&DataKey::Caller(caller.clone()));
    }
}

pub fn is_caller(env: &Env, caller: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Caller(caller.clone()))
        .unwrap_or(false)
}
```

- [ ] **Step 3: Wire modules in `lib.rs` and rewrite storage call sites to use helpers**

At top of `lib.rs`:

```rust
#![no_std]
//! LumAgg arb vault: ...
//! (keep the existing crate-level auth-pitfall docs)

mod admin;
mod auth;
mod execute;
mod storage;
mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};
use lumagg_contract_types::SubRoute;

pub use types::AggregatorContract;

#[contract]
pub struct VaultContract;
```

For this task only: add `mod types; mod storage;` first, keep admin/auth/execute bodies still in `lib.rs` if preferred — **or** create stub modules that `pub use` nothing yet. Prefer completing types+storage and updating `lib.rs` call sites immediately:

Replace:
- `env.storage().instance().has(&DataKey::Admin)` → `storage::has_admin(&env)`
- `env.storage().instance().set(&DataKey::Admin, &admin)` → `storage::set_admin(&env, &admin)`
- `env.storage().instance().get(&DataKey::Admin).expect(...)` → `storage::get_admin(&env)`
- caller persistent get/set/remove → `storage::is_caller` / `storage::set_caller`

Remove local `DataKey` and `AggregatorContract` trait from `lib.rs`.

- [ ] **Step 4: Compile + test**

```bash
cargo test -p vault-contract
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add contracts/vault/src/types.rs contracts/vault/src/storage.rs contracts/vault/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor(vault): extract types and storage modules

EOF
)"
```

---

### Task 3: Extract vault `auth.rs` + `admin.rs`

**Files:**
- Create: `contracts/vault/src/auth.rs`
- Create: `contracts/vault/src/admin.rs`
- Modify: `contracts/vault/src/lib.rs`

- [ ] **Step 1: Create `auth.rs`**

```rust
use crate::storage;
use soroban_sdk::{Address, Env};

pub fn require_admin(env: &Env) -> Address {
    let admin = storage::get_admin(env);
    admin.require_auth();
    admin
}

pub fn require_caller(env: &Env, caller: &Address) {
    caller.require_auth();
    assert!(storage::is_caller(env, caller), "caller not authorized");
}
```

- [ ] **Step 2: Create `admin.rs` with moved admin methods**

Move the bodies of `initialize`, `upgrade`, `admin`, `add_caller`, `remove_caller`, `is_caller`, `admin_withdraw` into free functions that take `Env` (and other args). Use `auth::require_admin` where admin auth was inline.

Example shapes:

```rust
use crate::{auth, storage};
use soroban_sdk::{token, Address, BytesN, Env};

pub fn initialize(env: Env, admin: Address) {
    if storage::has_admin(&env) {
        panic!("Already initialized");
    }
    storage::set_admin(&env, &admin);
}

pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
    let _admin = auth::require_admin(&env);
    env.deployer().update_current_contract_wasm(new_wasm_hash);
}

pub fn admin(env: Env) -> Address {
    storage::get_admin(&env)
}

pub fn add_caller(env: Env, caller: Address) {
    let _admin = auth::require_admin(&env);
    storage::set_caller(&env, &caller, true);
}

pub fn remove_caller(env: Env, caller: Address) {
    let _admin = auth::require_admin(&env);
    storage::set_caller(&env, &caller, false);
}

pub fn is_caller(env: Env, caller: Address) -> bool {
    storage::is_caller(&env, &caller)
}

pub fn admin_withdraw(env: Env, token: Address, to: Address, amount: i128) {
    let _admin = auth::require_admin(&env);
    assert!(amount > 0, "amount must be positive");
    let vault = env.current_contract_address();
    token::Client::new(&env, &token).transfer(&vault, &to, &amount);
}
```

**Critical:** keep panic strings and assert messages identical.

- [ ] **Step 3: Thin `lib.rs` admin entrypoints**

```rust
#[contractimpl]
impl VaultContract {
    pub fn initialize(env: Env, admin: Address) {
        admin::initialize(env, admin)
    }
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        admin::upgrade(env, new_wasm_hash)
    }
    pub fn admin(env: Env) -> Address {
        admin::admin(env)
    }
    pub fn add_caller(env: Env, caller: Address) {
        admin::add_caller(env, caller)
    }
    pub fn remove_caller(env: Env, caller: Address) {
        admin::remove_caller(env, caller)
    }
    pub fn is_caller(env: Env, caller: Address) -> bool {
        admin::is_caller(env, caller)
    }
    pub fn admin_withdraw(env: Env, token: Address, to: Address, amount: i128) {
        admin::admin_withdraw(env, token, to, amount)
    }
    // deposit + execute_round_trip still here until Task 4
}
```

- [ ] **Step 4: Test**

```bash
cargo test -p vault-contract
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add contracts/vault/src/auth.rs contracts/vault/src/admin.rs contracts/vault/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor(vault): extract auth and admin modules

EOF
)"
```

---

### Task 4: Extract vault `execute.rs` + move tests

**Files:**
- Create: `contracts/vault/src/execute.rs`
- Create: `contracts/vault/src/tests.rs`
- Modify: `contracts/vault/src/lib.rs`

- [ ] **Step 1: Create `execute.rs`**

Move `deposit` and `execute_round_trip` bodies here. Use `auth::require_caller` for the start of `execute_round_trip` (same order: `caller.require_auth()` then allowlist assert — `require_caller` already does both).

Preserve:
- `i128::MAX` approve
- client-provided `allowance_expiration_ledger`
- transfer vault→caller → aggregator call → `transfer_from` reclaim
- assert messages (`amount_in must be positive`, `min_amount_out below principal`, `allowance expiration in the past`)

```rust
use crate::{auth, types::AggregatorContractClient};
use lumagg_contract_types::SubRoute;
use soroban_sdk::{token, Address, Env, Vec};

pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
    from.require_auth();
    assert!(amount > 0, "amount must be positive");
    let vault = env.current_contract_address();
    token::Client::new(&env, &token).transfer(&from, &vault, &amount);
}

pub fn execute_round_trip(
    env: Env,
    caller: Address,
    aggregator: Address,
    base_token: Address,
    bridge_token: Address,
    amount_in: i128,
    leg_out: Vec<SubRoute>,
    leg_back: Vec<SubRoute>,
    min_amount_out: i128,
    allowance_expiration_ledger: u32,
) -> i128 {
    auth::require_caller(&env, &caller);
    assert!(amount_in > 0, "amount_in must be positive");
    assert!(min_amount_out >= amount_in, "min_amount_out below principal");
    assert!(
        allowance_expiration_ledger >= env.ledger().sequence(),
        "allowance expiration in the past"
    );

    let vault = env.current_contract_address();
    let base_client = token::Client::new(&env, &base_token);

    base_client.approve(&caller, &vault, &i128::MAX, &allowance_expiration_ledger);
    base_client.transfer(&vault, &caller, &amount_in);

    let agg = AggregatorContractClient::new(&env, &aggregator);
    let base_total = agg.round_trip_swap(
        &caller,
        &base_token,
        &bridge_token,
        &amount_in,
        &leg_out,
        &leg_back,
        &min_amount_out,
    );

    base_client.transfer_from(&vault, &caller, &vault, &base_total);
    base_total
}
```

- [ ] **Step 2: Finish thin facade in `lib.rs`**

```rust
pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
    execute::deposit(env, from, token, amount)
}

pub fn execute_round_trip(
    env: Env,
    caller: Address,
    aggregator: Address,
    base_token: Address,
    bridge_token: Address,
    amount_in: i128,
    leg_out: Vec<SubRoute>,
    leg_back: Vec<SubRoute>,
    min_amount_out: i128,
    allowance_expiration_ledger: u32,
) -> i128 {
    execute::execute_round_trip(
        env,
        caller,
        aggregator,
        base_token,
        bridge_token,
        amount_in,
        leg_out,
        leg_back,
        min_amount_out,
        allowance_expiration_ledger,
    )
}
```

Keep the crate-level doc comments on `lib.rs` (auth pitfall). Optionally keep a short doc comment on the facade `execute_round_trip` pointing to crate docs.

- [ ] **Step 3: Move `#[cfg(test)] mod tests { ... }` body into `tests.rs`**

`tests.rs` starts with:

```rust
use {
    super::*,
    aggregator_contract::AggregatorContract,
    lumagg_contract_types::{DexType, SubRoute, SwapStep},
    soroban_sdk::{testutils::Address as _, token, vec, Address, Env},
};
```

Cut-paste the existing helpers and two tests unchanged. In `lib.rs` keep only:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Test**

```bash
cargo test -p vault-contract
```

Expected: PASS both tests.

- [ ] **Step 5: Commit**

```bash
git add contracts/vault/src/
git commit -m "$(cat <<'EOF'
refactor(vault): extract execute module and relocate tests

EOF
)"
```

---

### Task 5: Vault checkpoint — structure sanity

**Files:**
- Verify: `contracts/vault/src/*`

- [ ] **Step 1: Confirm layout**

```bash
ls -1 contracts/vault/src/
```

Expected files:

```
admin.rs
auth.rs
execute.rs
lib.rs
storage.rs
tests.rs
types.rs
```

- [ ] **Step 2: Confirm `lib.rs` has no business bodies**

`lib.rs` should only contain docs, `mod` lines, `use`, `#[contract]`, and thin `#[contractimpl]` delegations. Grep for panic/assert in production code outside modules:

```bash
rg -n "panic!|assert!" contracts/vault/src/lib.rs
```

Expected: no matches (panics live in admin/execute/storage).

- [ ] **Step 3: Final vault test + workspace touch**

```bash
cargo test -p vault-contract
cargo test -p aggregator-contract --lib -- --test-threads=1 2>&1 | tail -30
```

Aggregator tests are unrelated but ensure the workspace still builds against the vault crate as a dependency of nothing for aggregator; vault is only a consumer of aggregator in tests. Building vault-contract is enough; optional aggregator check:

```bash
cargo check -p vault-contract -p aggregator-contract
```

Expected: success.

- [ ] **Step 4: Commit only if you fixed leftovers; otherwise no commit**

---

### Task 6: Aggregator baseline + extract support modules

**Files:**
- Create: `contracts/aggregator/src/storage.rs`
- Create: `contracts/aggregator/src/auth.rs`
- Create: `contracts/aggregator/src/math.rs`
- Create: `contracts/aggregator/src/validate.rs`
- Create: `contracts/aggregator/src/events.rs`
- Modify: `contracts/aggregator/src/lib.rs`

Do **not** move `swap` / `round_trip_swap` / `execute_step*` yet in this task.

- [ ] **Step 1: Baseline aggregator tests**

```bash
cargo test -p aggregator-contract
```

Expected: all PASS (many snapshot tests under `test_snapshots/`).

- [ ] **Step 2: Extract `storage.rs` + `auth.rs`**

Same pattern as vault: `DataKey::Admin`, `has_admin` / `get_admin` / `set_admin`, `require_admin`.

- [ ] **Step 3: Extract `math.rs`**

Move unchanged:

- `soroswap_fee`
- `soroswap_get_amount_out`
- `scale_sub_routes_to_total`

Make them `pub(crate)`.

- [ ] **Step 4: Extract `validate.rs`**

Move `validate_sub_routes` out of the `impl` into `pub(crate) fn validate_sub_routes(...)`. Update call sites inside remaining `impl` methods to `validate::validate_sub_routes(...)`.

- [ ] **Step 5: Extract `events.rs`**

Add helpers that emit the exact current events:

```rust
// swap event topics/payload must stay identical:
// (Symbol::new(&env, "swap"),) + (user, token_in, token_out, total_in, total_output, sub_routes.len() as u32)

// round-trip:
// (Symbol::new(&env, "rt"),) + (user, base_token, bridge_token, amount_in, base_total, leg_counter, is_split)
```

Replace inline `env.events().publish(...)` in `swap` / `round_trip_swap` with these helpers.

- [ ] **Step 6: Test**

```bash
cargo test -p aggregator-contract
```

Expected: PASS including snapshot tests. If a snapshot fails, you changed event/order semantics — revert the helper to match exactly.

- [ ] **Step 7: Commit**

```bash
git add contracts/aggregator/src/
git commit -m "$(cat <<'EOF'
refactor(aggregator): extract storage, auth, math, validate, events

EOF
)"
```

---

### Task 7: Aggregator extract `invoke` + flow modules

**Files:**
- Create: `contracts/aggregator/src/invoke.rs`
- Create: `contracts/aggregator/src/admin.rs`
- Create: `contracts/aggregator/src/swap.rs`
- Create: `contracts/aggregator/src/round_trip.rs`
- Optional create: `contracts/aggregator/src/split.rs` (only if split helpers cleanly separate; otherwise keep split logic inside `swap.rs` / `math.rs`)
- Create: `contracts/aggregator/src/tests.rs` (move `mod test` body)
- Modify: `contracts/aggregator/src/lib.rs`

- [ ] **Step 1: Move DEX execution into `invoke.rs`**

Move:

- `comet_approval_ledger`
- `dex_tag`
- `execute_step`
- `execute_step_inner`
- (and any helpers only used by them)

Keep call order and auth entries identical.

- [ ] **Step 2: Move `execute_sub_routes` / `execute_path` into `swap.rs` (or `invoke.rs` if tightly coupled)**

Prefer:

- path/step invoke → `invoke.rs`
- multi-route orchestration used by both swap and round_trip → `swap.rs` as `pub(crate) fn execute_sub_routes`

- [ ] **Step 3: Move `swap` body to `swap.rs`, `round_trip_swap` to `round_trip.rs`, admin methods to `admin.rs`**

`lib.rs` becomes thin delegations only.

- [ ] **Step 4: Relocate tests to `tests.rs`**

Keep one file first (aggregator tests are huge). Do not split into `tests/*` unless the single move is already green and time remains.

- [ ] **Step 5: Test**

```bash
cargo test -p aggregator-contract
cargo test -p vault-contract
```

Expected: both packages PASS.

- [ ] **Step 6: Commit**

```bash
git add contracts/aggregator/src/
git commit -m "$(cat <<'EOF'
refactor(aggregator): extract invoke, admin, swap, and round_trip modules

EOF
)"
```

---

### Task 8: Final verification + stop point

**Files:** none new

- [ ] **Step 1: Full contract package tests**

```bash
cargo test -p aggregator-contract -p vault-contract
```

Expected: all PASS.

- [ ] **Step 2: Confirm no behavior-facing churn**

```bash
git diff main -- contracts/shared-types contracts/order-escrow
```

Expected: empty (this plan should not touch them).

- [ ] **Step 3: Document stop**

Do **not** start `order-escrow` in this plan. Open a follow-up issue/plan using the same module template (`types` / `storage` / `auth` / `events` / `limit_orders` / `dca_orders` / `fill` / `tests`).

- [ ] **Step 4: Optional summary commit** only if docs need a pointer

```bash
# only if you add a short note to docs/contracts-deployment.md about module layout
```

Otherwise stop.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Split vault first | Tasks 1–5 |
| vault types/storage/auth/admin/execute/tests | Tasks 2–4 |
| Preserve signatures/storage/events/auth | All tasks (explicit asserts + existing tests) |
| Aggregator support modules | Task 6 |
| Aggregator flow modules (swap/round_trip/invoke) | Task 7 |
| Defer order-escrow | Task 8 |
| Compile/test after each extraction | Every task |
| No frontend / analytics / schema changes | Out of scope |

## Notes for the implementing agent

- Prefer **move code, then fix imports** over rewriting logic.
- Panic / assert string literals are part of test surface for some contracts — keep them identical.
- Aggregator has `test_snapshots/`; treat snapshot diffs as regressions unless you intentionally changed XDR/event encoding (you must not).
- Do not run `stellar contract build` / mainnet upgrade as part of this refactor unless the user explicitly asks.

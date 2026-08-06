## Goal

Refactor the Soroban contracts from single-file `lib.rs` implementations into
small, focused modules without changing external behavior.

This design covers:

- `contracts/vault`
- `contracts/aggregator`
- later application to `contracts/order-escrow`

This design explicitly does **not** include behavior changes, storage schema
changes, event format changes, or contract interface changes.

## Non-Goals

- No changes to public contract method names, parameters, or return values
- No changes to storage keys or persistence layout
- No changes to emitted event topics or payload shapes
- No changes to authorization semantics
- No optimizer-style refactors that alter execution order
- No router, front-end, or analytics work

## Constraints

- Contract behavior must remain byte-for-byte compatible at the interface level
  even if internal file organization changes.
- Existing tests must continue to pass with the same intent.
- The refactor should be reviewable in small steps.
- Each contract should compile after its own split before moving to the next.

## Recommended Rollout

1. Split `vault` first
2. Reuse the same boundaries for `aggregator`
3. Split `order-escrow` last, after the pattern is proven

Rationale:

- `vault` is the smallest contract and is a safe template
- `aggregator` is the most important contract, so it benefits from a validated
  structure before refactor
- `order-escrow` is large and test-heavy, so it should follow an established
  pattern

## Approach Options

### Option A: Minimal split per contract

Move code into a few files such as `storage`, `auth`, `execute`, and `tests`,
keeping most business logic grouped together.

Pros:

- Lowest implementation risk
- Fastest to complete
- Smallest diff

Cons:

- Large business modules remain large
- `aggregator` may still be hard to audit

### Option B: Focused module split by responsibility

Split each contract into modules aligned to responsibility boundaries, such as
`storage`, `auth`, `events`, `math`, `validate`, `swap`, and `round_trip`.

Pros:

- Best long-term maintainability
- Clear audit and review boundaries
- Easier future bugfixes and feature work

Cons:

- Larger diff than Option A
- Requires careful module design

### Option C: Full split plus logic cleanup

Split files and also normalize internal helpers, deduplicate patterns, and
reshape some private flows.

Pros:

- Cleanest end state

Cons:

- Highest regression risk
- Too easy to accidentally change behavior
- Harder to review

## Recommendation

Use **Option B** with a strict behavior-preservation rule.

That means:

- split by responsibility
- allow private helper extraction
- do not change interface, storage, events, or auth behavior

## Target Layout: `contracts/vault`

Recommended files:

- `src/lib.rs`
- `src/types.rs`
- `src/storage.rs`
- `src/auth.rs`
- `src/admin.rs`
- `src/execute.rs`
- `src/tests.rs`

### `vault/src/lib.rs`

Responsibilities:

- `mod` declarations
- top-level imports
- contract type declaration
- thin `#[contractimpl]` entrypoints that delegate to module functions

Should not contain:

- storage reads/writes inline
- auth logic inline
- business logic bodies
- test helpers

### `vault/src/types.rs`

Responsibilities:

- `#[contractclient]` trait for the aggregator client
- internal `#[contracttype]` definitions

Should remain small and declarative.

### `vault/src/storage.rs`

Responsibilities:

- `DataKey`
- `get_admin()`
- `set_admin()`
- caller allowlist helpers

Rules:

- no auth checks
- no side effects beyond storage

### `vault/src/auth.rs`

Responsibilities:

- `require_admin()`
- `require_caller()`

Rules:

- perform checks only
- do not mutate storage

### `vault/src/admin.rs`

Responsibilities:

- `initialize`
- `upgrade`
- `admin`
- `add_caller`
- `remove_caller`
- `is_caller`
- `admin_withdraw`

Rules:

- may call `auth` and `storage`
- no test helpers

### `vault/src/execute.rs`

Responsibilities:

- `deposit`
- `execute_round_trip`

Rules:

- preserve current authorization flow
- preserve current allowance expiration semantics
- preserve current transfer ordering

### `vault/src/tests.rs`

Responsibilities:

- mock pool / mock aggregator setup
- token factories
- test environment helpers
- all contract tests

Goal:

remove test-only code from production modules.

## Target Layout: `contracts/aggregator`

Recommended files:

- `src/lib.rs`
- `src/types.rs`
- `src/storage.rs`
- `src/auth.rs`
- `src/events.rs`
- `src/math.rs`
- `src/validate.rs`
- `src/invoke.rs`
- `src/admin.rs`
- `src/swap.rs`
- `src/split.rs`
- `src/round_trip.rs`
- `src/tests/mod.rs` and split test files if needed

### `aggregator/src/lib.rs`

Responsibilities:

- `mod` declarations
- contract type declaration
- thin `#[contractimpl]` entrypoints

### `aggregator/src/types.rs`

Responsibilities:

- internal shared contract types used across modules
- keep route/event/shared structures centralized

Note:

If this file grows too large during implementation, split into
`types_route.rs`, `types_event.rs`, and `types_storage.rs`, but do not do that
preemptively.

### `aggregator/src/storage.rs`

Responsibilities:

- `DataKey`
- admin storage helpers

### `aggregator/src/auth.rs`

Responsibilities:

- admin auth helpers
- any reusable auth-context helpers

### `aggregator/src/events.rs`

Responsibilities:

- emit helper for `swap`
- emit helper for round-trip event

Reason:

event shape is part of the analytics contract between contracts and indexers,
so it should have a dedicated boundary.

### `aggregator/src/math.rs`

Responsibilities:

- `soroswap_fee`
- `soroswap_get_amount_out`
- route scaling helpers
- any pure calculation helper

Rules:

- pure functions only
- no `Env` unless absolutely necessary for Soroban collection construction

### `aggregator/src/validate.rs`

Responsibilities:

- sub-route validation
- token continuity checks
- input amount checks

Reason:

validation should be auditable independently from execution.

### `aggregator/src/invoke.rs`

Responsibilities:

- adapter-specific contract invocation helpers
- one place for external DEX contract call mechanics

Reason:

separate "how to call a pool" from "when and why to call it."

### `aggregator/src/admin.rs`

Responsibilities:

- `initialize`
- `upgrade`
- `admin`

### `aggregator/src/swap.rs`

Responsibilities:

- single-leg / multi-hop swap flow
- pull input from user
- execute route
- enforce output minimum
- transfer output back

### `aggregator/src/split.rs`

Responsibilities:

- split-route orchestration
- allocation and aggregation helpers specific to split execution

If implementation reveals that split behavior remains naturally embedded in
`swap.rs`, this module may hold only split-specific helpers rather than a full
entrypoint body.

### `aggregator/src/round_trip.rs`

Responsibilities:

- `round_trip_swap`
- leg-out execution
- rescale of leg-back weights
- final base-token return checks

Reason:

round-trip logic is conceptually separate from standard swap logic and should
be reviewed in isolation.

### `aggregator/src/tests/*`

Recommended split:

- `tests/helpers.rs`
- `tests/swap.rs`
- `tests/split.rs`
- `tests/round_trip.rs`
- `tests/admin.rs`

This can stay as one `tests.rs` in the first pass if file movement alone is
already large enough.

## `order-escrow` Direction

Do not start with `order-escrow`.

When reached, use the same pattern with domain-focused modules such as:

- `types`
- `storage`
- `auth`
- `events`
- `limit_orders`
- `dca_orders`
- `fill`
- `admin` if needed
- `tests`

The most important boundary there is separating:

- order lifecycle
- fill execution
- DCA-specific scheduling logic

## Invariants That Must Not Change

The refactor must preserve:

1. public method signatures
2. storage keys and value shapes
3. event topic names and payload ordering
4. authorization requirements
5. transfer ordering and side effects
6. current arithmetic behavior, including rounding behavior
7. current failure semantics where tests depend on them

## Validation Strategy

For each contract refactor:

1. move code without semantic edits where possible
2. compile immediately after module extraction
3. run contract tests for that contract
4. compare event assertions and critical auth-path tests
5. only then move to the next contract

Suggested checkpoints:

- after `vault` split
- after `aggregator` support-module split
- after `aggregator` flow-module split
- after `order-escrow` split

## Implementation Scope for First Pass

Included:

- file/module split
- private helper extraction
- test file relocation
- import cleanup caused by module boundaries

Excluded:

- renaming public APIs
- changing event schemas
- changing storage schema
- changing contract behavior
- opportunistic refactors unrelated to modularization

## Recommended First Implementation

First implementation should stop after:

1. splitting `vault`
2. verifying tests
3. optionally beginning `aggregator` support-module split if the `vault` pass is clean

This keeps the first code review narrow and lets the project validate the
pattern before touching the largest contract.

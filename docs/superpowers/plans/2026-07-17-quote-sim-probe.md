# Quote vs Sim Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone diagnostic binary that randomly samples trade paths, compares quote-api local math vs on-chain hop estimates / full simulation *at the same moment*, and reports which hop first diverges — without touching the arb hot path.

**Architecture:** A new `quote-sim-probe` binary in `crates/arbitrage` reuses existing quote-api client + `dex_adapters::on_chain_quote` + optional vault round-trip simulate. It samples one-leg and/or round-trip routes from configured bridge tokens, prints structured JSONL reports (per-hop local vs chain, gap_bps), and exits non-zero when median gap exceeds a threshold. No Redis stream, no arb enqueue.

**Tech Stack:** Rust, reqwest (quote-api), SorobanRpc (`estimate_swap` / Soroswap reserves), existing `arbitrage::{bridge,quote_client,prepare,invoke}` helpers.

---

## File structure

| File | Role |
|------|------|
| `crates/arbitrage/src/bin/quote_sim_probe.rs` | CLI entry: sample loop, report JSONL |
| `crates/arbitrage/src/probe.rs` | Pure helpers: gap_bps, sample selection, report structs (unit-tested) |
| `crates/arbitrage/Cargo.toml` | Register `[[bin]] quote-sim-probe` |
| `docs/arb-operator.md` | Short “Quote/sim probe” subsection |
| `scripts/quote-sim-probe.sh` | Convenience wrapper with production-ish env defaults |

**Out of scope:** wiring into `arb-scanner`, Telegram alerts for probe, API changes, `apply_on_chain_hop_validation` in the quote hot path.

---

### Task 1: Pure probe helpers + failing tests

**Files:**
- Create: `crates/arbitrage/src/probe.rs`
- Modify: `crates/arbitrage/src/lib.rs` (add `pub mod probe;`)
- Test: unit tests inside `probe.rs`

- [ ] **Step 1: Write failing tests for gap and first-diverging hop**

```rust
// crates/arbitrage/src/probe.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_bps_matches_20bps_fixture() {
        let amount_in = 100_000_000u128;
        let local = 100_143_095u128;
        let chain = 99_942_226u128;
        let gap = hop_gap_bps(amount_in, local, chain);
        assert!((19..=21).contains(&gap), "gap={gap}");
    }

    #[test]
    fn first_diverging_hop_picks_earliest_above_threshold() {
        let hops = vec![
            HopCompare {
                index: 0,
                source: "soroswap".into(),
                pool: "P0".into(),
                amount_in: 100_000_000,
                local_out: 50_000_000,
                chain_out: Some(50_000_000),
            },
            HopCompare {
                index: 1,
                source: "aquarius".into(),
                pool: "P1".into(),
                amount_in: 50_000_000,
                local_out: 100_200_000,
                chain_out: Some(99_900_000),
            },
        ];
        let idx = first_diverging_hop(&hops, 5).expect("should find hop 1");
        assert_eq!(idx, 1);
    }

    #[test]
    fn sample_round_robin_bridges_is_deterministic_with_seed() {
        let bridges = vec!["A".into(), "B".into(), "C".into()];
        let a = pick_bridges(&bridges, 5, 42);
        let b = pick_bridges(&bridges, 5, 42);
        assert_eq!(a, b);
        assert_eq!(a.len(), 5);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p arbitrage --lib probe:: -- --nocapture`

Expected: FAIL (module / types missing)

- [ ] **Step 3: Minimal implementation**

```rust
//! Offline quote-vs-chain comparison helpers (no arb hot path).

use crate::scanner::compute_profit_bps;

#[derive(Debug, Clone)]
pub struct HopCompare {
    pub index: usize,
    pub source: String,
    pub pool: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_out: Option<u128>,
}

/// (local_out − chain_out) / amount_in in bps, using local as the reference
/// notional for the hop input. Positive ⇒ local optimistic vs chain.
pub fn hop_gap_bps(amount_in: u128, local_out: u128, chain_out: u128) -> i64 {
    let local_bps = compute_profit_bps(amount_in, local_out);
    let chain_bps = compute_profit_bps(amount_in, chain_out);
    local_bps.saturating_sub(chain_bps)
}

pub fn first_diverging_hop(hops: &[HopCompare], threshold_bps: i64) -> Option<usize> {
    for h in hops {
        let Some(chain) = h.chain_out else {
            return Some(h.index);
        };
        if hop_gap_bps(h.amount_in, h.local_out, chain).abs() >= threshold_bps {
            return Some(h.index);
        }
    }
    None
}

/// Deterministic sampling with a simple LCG seeded RNG (no extra deps).
pub fn pick_bridges(bridges: &[String], count: usize, seed: u64) -> Vec<String> {
    if bridges.is_empty() || count == 0 {
        return Vec::new();
    }
    let mut state = seed.max(1);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        let idx = (state as usize) % bridges.len();
        out.push(bridges[idx].clone());
    }
    out
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeSampleReport {
    pub mode: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_path_out: Option<u128>,
    pub gap_bps: Option<i64>,
    pub first_bad_hop: Option<usize>,
    pub hops: Vec<HopCompareReport>,
    pub simulate_out: Option<u128>,
    pub simulate_gap_bps: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HopCompareReport {
    pub index: usize,
    pub source: String,
    pub pool: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_out: Option<u128>,
    pub gap_bps: Option<i64>,
}

impl From<&HopCompare> for HopCompareReport {
    fn from(h: &HopCompare) -> Self {
        let gap_bps = h.chain_out.map(|c| hop_gap_bps(h.amount_in, h.local_out, c));
        Self {
            index: h.index,
            source: h.source.clone(),
            pool: h.pool.clone(),
            amount_in: h.amount_in,
            local_out: h.local_out,
            chain_out: h.chain_out,
            gap_bps,
        }
    }
}
```

Also add to `lib.rs`:

```rust
pub mod probe;
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p arbitrage --lib probe:: -- --nocapture`

Expected: all `probe::tests` PASS

- [ ] **Step 5: Commit**

```bash
git add crates/arbitrage/src/probe.rs crates/arbitrage/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(arbitrage): add quote/sim probe helpers

EOF
)"
```

---

### Task 2: Per-leg hop compare against quote-api + on-chain

**Files:**
- Create: `crates/arbitrage/src/bin/quote_sim_probe.rs` (skeleton + one-leg mode)
- Modify: `crates/arbitrage/Cargo.toml`

- [ ] **Step 1: Register binary**

In `crates/arbitrage/Cargo.toml` add:

```toml
[[bin]]
name = "quote-sim-probe"
path = "src/bin/quote_sim_probe.rs"
```

- [ ] **Step 2: Implement one-leg compare (reuse quote-hop-compare logic)**

```rust
//! Independent quote-api vs on-chain hop probe (does not run inside arb-scanner).
//!
//! Usage:
//!   ARB_QUOTE_API_URLS=http://127.0.0.1:3100 RPC_URL=http://127.0.0.1:8003 \
//!     cargo run -p arbitrage --bin quote-sim-probe -- \
//!     --mode one-leg --token-in CAS3...OWMA --token-out CCW67...MI75 --amount-in 100000000
//!
//! Random / batch:
//!   cargo run -p arbitrage --bin quote-sim-probe -- \
//!     --mode round-trip --samples 20 --seed 1 --amount-in 100000000 --jsonl

use {
    anyhow::{Context, Result},
    arbitrage::{
        probe::{first_diverging_hop, hop_gap_bps, HopCompare, HopCompareReport, ProbeSampleReport},
        scanner::compute_profit_bps,
    },
    dex_adapters::{on_chain_quote, rpc::SorobanRpc},
    serde_json::Value,
    std::env,
};

async fn compare_quote_leg(
    quote_api: &str,
    rpc: &SorobanRpc,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
    threshold_bps: i64,
) -> Result<ProbeSampleReport> {
    let url = format!(
        "{quote_api}/api/v1/quote?token_in={token_in}&token_out={token_out}&amount_in={amount_in}&prefer_soroban=1&max_splits=1"
    );
    let body: Value = reqwest::get(&url).await?.json().await?;
    if body["success"].as_bool() != Some(true) {
        return Ok(ProbeSampleReport {
            mode: "one-leg".into(),
            token_in: token_in.into(),
            token_out: token_out.into(),
            amount_in,
            local_out: 0,
            chain_path_out: None,
            gap_bps: None,
            first_bad_hop: None,
            hops: vec![],
            simulate_out: None,
            simulate_gap_bps: None,
            error: Some(format!("quote failed: {body}")),
        });
    }
    let data = &body["data"];
    let local_out: u128 = data["expected_output"].as_str().unwrap_or("0").parse()?;

    // Probe only the first sub_route (max_splits=1).
    let sub = &data["sub_routes"].as_array().context("sub_routes")?[0];
    let sources: Vec<String> = sub["dex_types"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let pools: Vec<String> = sub["pool_addresses"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let tokens: Vec<String> = sub["path"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let in_indices: Vec<u32> = sub["in_indices"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();
    let out_indices: Vec<u32> = sub["out_indices"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();

    // Reconstruct per-hop *local* outs by chaining: we only have sub total local_out.
    // Approximate local hop outs by running on-chain sequentially for chain, and
    // attributing the full-path local only at the end. For first-hop localization
    // we compare chain hop outs against a second local pass is not available from
    // the API — so report chain hop chain, and path-level local vs chain total.
    //
    // Better localization: use path_amount_out_on_chain for full path, and also
    // per-hop hop_amount_out_on_chain with the *chain* running amount; local hop
    // outs are unknown from API. Document this limitation: path-level gap is
    // authoritative; hop-level marks which hop's *chain* step failed or shrank.
    let mut hops = Vec::new();
    let mut current = amount_in;
    let mut chain_path_out = Some(amount_in);
    for i in 0..sources.len() {
        let chain_out = on_chain_quote::hop_amount_out_on_chain(
            rpc,
            &sources[i],
            &pools[i],
            &tokens[i],
            &tokens[i + 1],
            in_indices[i],
            out_indices[i],
            current,
        )
        .await?;
        // local_out per hop unknown from public API → use 0 placeholder only when
        // we cannot attribute; for path report we still have total local_out.
        hops.push(HopCompare {
            index: i,
            source: sources[i].clone(),
            pool: pools[i].clone(),
            amount_in: current,
            local_out: 0, // filled only for last hop as path residual if needed
            chain_out,
        });
        match chain_out {
            Some(v) if v > 0 => current = v,
            _ => {
                chain_path_out = None;
                break;
            }
        }
    }
    if chain_path_out.is_some() {
        chain_path_out = Some(current);
        if let Some(last) = hops.last_mut() {
            last.local_out = local_out;
        }
    }

    let gap_bps = chain_path_out.map(|c| hop_gap_bps(amount_in, local_out, c));
    let first_bad = first_diverging_hop(&hops, threshold_bps);

    Ok(ProbeSampleReport {
        mode: "one-leg".into(),
        token_in: token_in.into(),
        token_out: token_out.into(),
        amount_in,
        local_out,
        chain_path_out,
        gap_bps,
        first_bad_hop: first_bad,
        hops: hops.iter().map(HopCompareReport::from).collect(),
        simulate_out: None,
        simulate_gap_bps: None,
        error: None,
    })
}
```

**Important note for implementer:** Public quote-api does **not** expose per-hop local amounts. Path-level `local_out` vs `chain_path_out` is the primary signal. Hop-level `chain_out` still shows where the chain path collapses; if we later need true per-hop local, add an optional `debug=1` hop breakdown to the API in a separate plan — **do not block this probe on that**.

- [ ] **Step 3: CLI `main` for one-leg**

Parse args with a tiny manual parser (or clap if already in workspace — prefer manual to avoid new deps):

```text
--mode one-leg|round-trip
--token-in C...
--token-out C...
--amount-in 100000000
--samples N          (round-trip mode)
--seed U64
--threshold-bps 10
--jsonl
--simulate           (round-trip only: also vault simulate)
```

Env: `ARB_QUOTE_API_URLS` / `QUOTE_API_URL`, `RPC_URL`, `ARB_BRIDGE_TOKENS`, `ARB_BASE_TOKENS` (same as arb).

- [ ] **Step 4: Manual smoke (local or against prod API)**

```bash
RPC_URL=http://127.0.0.1:8003 \
ARB_QUOTE_API_URLS=http://127.0.0.1:3100 \
cargo run -p arbitrage --bin quote-sim-probe -- \
  --mode one-leg \
  --token-in CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA \
  --token-out CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 \
  --amount-in 100000000 --jsonl
```

Expected: one JSON line with `local_out`, `chain_path_out`, `gap_bps`.

- [ ] **Step 5: Commit**

```bash
git add crates/arbitrage/Cargo.toml crates/arbitrage/src/bin/quote_sim_probe.rs
git commit -m "$(cat <<'EOF'
feat(arbitrage): add quote-sim-probe one-leg mode

EOF
)"
```

---

### Task 3: Round-trip mode (quote both legs + optional full simulate)

**Files:**
- Modify: `crates/arbitrage/src/bin/quote_sim_probe.rs`
- Reuse: `arbitrage::{bridge::quote_round_trip, config::ArbConfig, context::ArbContext, prepare, invoke}`

- [ ] **Step 1: Round-trip sample function**

For each bridge:

1. `quote_round_trip(ctx, base, bridge, amount_in)` via existing helper.
2. For **each leg** (`leg_out`, `leg_back`), run the same hop on-chain compare as Task 2 (using the leg’s `sub_orders[0]` path metadata from `LegQuote` / step_sets — prefer reconstructing from `quote.leg_out.route` + indices via `get_pool_indices` **or** re-fetch one-leg quote and compare; simplest: call `compare_quote_leg` twice with the quoted amounts).
3. If `--simulate`: build `execute_round_trip` op (copy pattern from `diag_simulate.rs`) and `prepare_transaction_xdr`; record `simulate_out` and `simulate_gap_bps` vs quote `amount_out`.

```rust
// Pseudocode for RT report aggregation
let quote = quote_round_trip(&ctx, &base, &bridge, amount_in).await?;
let out_report = compare_quote_leg(..., base, bridge, amount_in, ...).await?;
let back_report = compare_quote_leg(..., bridge, base, quote.leg_out.route.total_expected_out, ...).await?;
// Combined gap: quote.amount_out vs simulate_out (if --simulate)
// Also print leg_out.gap_bps and leg_back.gap_bps separately in JSON
```

Extend `ProbeSampleReport` **or** add `RoundTripProbeReport`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoundTripProbeReport {
    pub bridge: String,
    pub amount_in: u128,
    pub quoted_out: u128,
    pub quoted_profit_bps: i64,
    pub leg_out: ProbeSampleReport,
    pub leg_back: ProbeSampleReport,
    pub simulate_out: Option<u128>,
    pub simulate_gap_bps: Option<i64>,
    pub error: Option<String>,
}
```

Put the struct in `probe.rs` with a small unit test that serializes to JSON containing `leg_out` / `leg_back`.

- [ ] **Step 2: Random bridge sampling loop**

```rust
let bridges = /* from ARB_BRIDGE_TOKENS or --bridges CSV */;
let picks = arbitrage::probe::pick_bridges(&bridges, samples, seed);
for b in picks {
    let report = run_round_trip_sample(...).await?;
    if jsonl {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        print_human(&report);
    }
}
```

- [ ] **Step 3: Exit code**

After N samples, if median `|gap_bps|` of samples with both local+chain > `--threshold-bps`, exit `1` (useful for CI / cron). Otherwise exit `0`.

- [ ] **Step 4: Smoke round-trip**

```bash
source /opt/.../deploy/arb.env  # or local env
cargo run -p arbitrage --bin quote-sim-probe -- \
  --mode round-trip --samples 5 --seed 7 --amount-in 100000000 \
  --threshold-bps 10 --jsonl --simulate
```

Expected: 5 JSON lines; some with large `leg_out.gap_bps` or `simulate_gap_bps` matching production discards.

- [ ] **Step 5: Commit**

```bash
git add crates/arbitrage/src/probe.rs crates/arbitrage/src/bin/quote_sim_probe.rs
git commit -m "$(cat <<'EOF'
feat(arbitrage): quote-sim-probe round-trip sampling

EOF
)"
```

---

### Task 4: Wrapper script + operator docs

**Files:**
- Create: `scripts/quote-sim-probe.sh`
- Modify: `docs/arb-operator.md` (new subsection under Monitoring)

- [ ] **Step 1: Wrapper**

```bash
#!/usr/bin/env bash
# Independent quote vs on-chain probe (does not affect arb-scanner).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export RPC_URL="${RPC_URL:-http://127.0.0.1:8003}"
export ARB_QUOTE_API_URLS="${ARB_QUOTE_API_URLS:-http://127.0.0.1:3100,http://127.0.0.1:3101,http://127.0.0.1:3102,http://127.0.0.1:3103}"
cd "$ROOT"
cargo run -q -p arbitrage --bin quote-sim-probe -- "$@"
```

- [ ] **Step 2: Document in arb-operator.md**

Add under Monitoring:

```markdown
### Quote vs on-chain probe (offline)

Does **not** run inside `lumagg-arb`. Samples quote-api routes and immediately
compares each hop to on-chain `estimate_swap` / fresh Soroswap reserves (and
optionally a full vault `simulateTransaction`).

```bash
./scripts/quote-sim-probe.sh --mode round-trip --samples 20 --seed 1 \
  --amount-in 100000000 --jsonl --simulate
```

Use when `avg_quote_sim_gap_bps` / quiet-window alerts fire. Path-level
`gap_bps` is authoritative; hop `chain_out` shows where the chain path shrinks.
```

- [ ] **Step 3: Commit**

```bash
git add scripts/quote-sim-probe.sh docs/arb-operator.md
git commit -m "$(cat <<'EOF'
docs: add offline quote-sim-probe usage

EOF
)"
```

---

## Self-review

1. **Spec coverage:** Independent random/sampled paths ✓; same-moment quote+chain ✓; no arb hot path ✓; localize hop when possible ✓ (path-level primary; hop local amounts deferred).
2. **Placeholders:** None intentional; hop local_out limitation is explicit, not TBD.
3. **Types:** `HopCompare` / `ProbeSampleReport` / `RoundTripProbeReport` defined before use in the binary.

## Execution handoff

Plan saved to `docs/superpowers/plans/2026-07-17-quote-sim-probe.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — run tasks in this session with checkpoints  

Which approach?

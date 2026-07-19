# Phase 3 — Testnet Aggregator + Escrow Deploy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship testnet-only deploy scripts for aggregator + order-escrow (with mainnet refuse), an env snippet writer, and a smoke checklist doc.

**Architecture:** Bash scripts mirror `contracts/vault/deploy.sh` / `contracts/aggregator/upgrade.sh` (build WASM, deploy, initialize, poll timeouts) but default to Test SDF Network and exit if asked to use Public Global mainnet. A thin wrapper runs both deploys and writes a local env file for API/indexer/keeper.

**Tech Stack:** bash, stellar CLI, cargo/stellar contract build, gitignore for local id/env files.

**Spec:** `docs/superpowers/specs/2026-07-19-phase3-testnet-deploy-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `contracts/aggregator/deploy-testnet.sh` | Deploy + `initialize(admin)` on testnet; refuse mainnet |
| `contracts/order-escrow/deploy-testnet.sh` | Deploy + `initialize(admin, aggregator)` on testnet; refuse mainnet |
| `scripts/deploy-limit-testnet.sh` | Orchestrate both + write `deploy/.env.limit-testnet.local` |
| `docs/limit-orders-testnet.md` | Operator smoke checklist |
| `.gitignore` | Ignore `.testnet-*-id` and `deploy/.env.limit-testnet.local` |

---

### Task 1: Shared mainnet-refuse + gitignore

**Files:**
- Modify: `.gitignore`
- Create (optional helper): `scripts/lib/refuse-mainnet.sh` **or** inline the same guard at top of each script

- [ ] **Step 1:** Add gitignore entries:

```
contracts/aggregator/.testnet-aggregator-id
contracts/order-escrow/.testnet-escrow-id
deploy/.env.limit-testnet.local
```

- [ ] **Step 2:** Document the refuse rule used by all scripts:

```bash
refuse_if_mainnet() {
  case "${NETWORK_PASSPHRASE}" in
    *"Public Global Stellar Network"*)
      echo "ERROR: This script is testnet-only. Refusing mainnet passphrase." >&2
      exit 1
      ;;
  esac
}
```

Defaults for all new scripts:

```bash
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
```

- [ ] **Step 3:** Commit `chore: gitignore testnet deploy local artifacts`

---

### Task 2: Aggregator `deploy-testnet.sh`

**Files:**
- Create: `contracts/aggregator/deploy-testnet.sh`
- Reference patterns: `contracts/vault/deploy.sh`, `contracts/aggregator/upgrade.sh`

- [ ] **Step 1:** Implement script that:
  1. Sets testnet defaults + calls refuse guard
  2. Requires `ADMIN` key identity + `ADMIN_G` (or derive from `stellar keys address`)
  3. If `AGGREGATOR` already set to a `C…` id: skip deploy, print reuse message, exit 0 after validating format
  4. Else: build/optimize aggregator WASM (same candidate path list style as vault)
  5. `stellar contract deploy` with `--rpc-url` / `--network-passphrase`
  6. Parse `C…` id; poll on submission timeout (copy poll helper from vault)
  7. `initialize --admin "$ADMIN_G"`
  8. Write id to `contracts/aggregator/.testnet-aggregator-id`
  9. Print testnet explorer link: `https://stellar.expert/explorer/testnet/contract/$ID`

- [ ] **Step 2:** `chmod +x` the script

- [ ] **Step 3:** Dry-run sanity (no network): `bash -n contracts/aggregator/deploy-testnet.sh` and a quick test that mainnet passphrase fails:

```bash
NETWORK_PASSPHRASE='Public Global Stellar Network ; September 2015' \
  bash contracts/aggregator/deploy-testnet.sh && exit 1 || true
```

Expected: exit 1 with refuse message (before needing keys is ideal — call refuse immediately after defaults).

- [ ] **Step 4:** Commit `feat(contracts): testnet-only aggregator deploy script`

---

### Task 3: Order-escrow `deploy-testnet.sh`

**Files:**
- Create: `contracts/order-escrow/deploy-testnet.sh`
- Package name: check `contracts/order-escrow/Cargo.toml` for correct `cargo build -p …` / wasm name

- [ ] **Step 1:** Same structure as aggregator script, plus:
  - Require `AGGREGATOR=C…` (or read from `contracts/aggregator/.testnet-aggregator-id`)
  - After deploy: `initialize --admin "$ADMIN_G" --aggregator "$AGGREGATOR"`
  - Save `contracts/order-escrow/.testnet-escrow-id`

- [ ] **Step 2:** `bash -n` + mainnet-refuse smoke

- [ ] **Step 3:** Commit `feat(contracts): testnet-only order-escrow deploy script`

---

### Task 4: Wrapper + env snippet

**Files:**
- Create: `scripts/deploy-limit-testnet.sh`
- Create: `deploy/.gitkeep` if `deploy/` needs to exist (only if not present)

- [ ] **Step 1:** Wrapper:
  1. Refuse mainnet
  2. Run aggregator deploy-testnet (or reuse `AGGREGATOR`)
  3. Export `AGGREGATOR` from output file
  4. Run escrow deploy-testnet
  5. Write `deploy/.env.limit-testnet.local` with RPC, passphrase, `KEEPER_NETWORK=testnet`, `AGGREGATOR_CONTRACT`, `ESCROW_CONTRACT`, suggested `INDEXER_DB_PATH`

- [ ] **Step 2:** `bash -n` + mainnet refuse

- [ ] **Step 3:** Commit `feat(scripts): orchestrate limit-order testnet deploy + env snippet`

---

### Task 5: Smoke doc + README pointers

**Files:**
- Create: `docs/limit-orders-testnet.md`
- Modify: `contracts/order-escrow/README.md` (short “Testnet deploy” section linking the doc/scripts)
- Optionally one-line link from `crates/limit-keeper/README.md`

- [ ] **Step 1:** Write checklist covering prerequisites, deploy, point env at services, `build_create` → index → list, optional keeper dry-run, explicit no-mainnet

- [ ] **Step 2:** Commit `docs: limit orders testnet deploy and smoke checklist`

---

### Task 6: Verify (offline)

- [ ] **Step 1:** `bash -n` all three scripts
- [ ] **Step 2:** Confirm each refuses mainnet passphrase without deploying
- [ ] **Step 3:** Confirm `.gitignore` covers id/env files
- [ ] **Step 4:** Do **not** require live deploy in CI (needs funded key); note manual deploy for operator

---

## Out of scope

- Live mainnet / production cutover
- Phase 3d UI
- Automated funded E2E in CI

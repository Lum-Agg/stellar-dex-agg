# Smart contract audit — scope & budget (Tranche 3)

**Grant line item:** **$16,000** (external firm or Stellar audit bank + remediation engineering).  
**Target completion:** Oct 1, 2026 (per [scf-resubmission-budget.md](./scf-resubmission-budget.md)).

## Contracts in scope

This application requests review of the two contracts supporting the current
mainnet product: the swap aggregator and the arbitrage vault. Order Escrow for
Limit/DCA remains testnet-only and is explicitly deferred from this engagement.

| Contract | Mainnet ID | Critical paths |
|----------|------------|----------------|
| **Aggregator** | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` | `swap`, `split_swap`, `round_trip_swap`; DEX CPI; event emission |
| **Arb vault** | `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN` | `execute_round_trip`, caller allowlist, `deposit`, `admin_withdraw`, CPI to aggregator |

Source: `contracts/aggregator/` and `contracts/vault/`.

## Deferred contract

Order Escrow is currently deployed only on testnet at
`CCI3U3P7MPZNCA5L7KWTXNS7H7KV6AIZQ6ZY2FEOZPHTJIAVCRYPKXTM`. It is not part of
this audit and must remain testnet-only. A separate audit covering authorization,
refunds, expiry reclaim, restricted venue guarantees, and cross-contract fund
flows is required before any mainnet Limit/DCA deployment.

## Out of scope (unless bundled)

- Off-chain `crates/arbitrage` (Rust bot — not on-chain)
- `api-server` / `market-data-worker`
- Order Escrow (`contracts/order-escrow`) — deferred to a separate pre-mainnet audit
- Third-party DEX contracts (Aquarius, Soroswap, etc.)

## What $16k typically buys

For **two Soroban WASM contracts** with CPI and fund flow:

| Item | Rough range (USD) |
|------|-------------------|
| Focused review (1 auditor, ~1–2 weeks) | **$12k – $18k** |
| Full dual-contract + remediation support | **$16k – $25k** |
| Big-4 / top-tier firm | **$30k+** |

The **$16k grant allocation** is intended for this focused two-contract scope.
Ask the Audit Bank to confirm remediation and retest coverage; do not reduce the
authorization, fund-flow, or CPI review depth to fit the budget.

The allocation is otherwise realistic for:

- Stellar-ecosystem auditors (e.g. firms that have done Soroban/SDF-adjacent work)
- **Stellar Community Fund audit bank** / panel-recommended vendors (if available for awarded projects)
- A focused review of the two in-scope contracts

Remediation (fix + re-audit critical/high) is included in the grant line — budget **~2–4 XLM WASM uploads** (~$20 XLM each on mainnet) separately from auditor fee.

## How to engage

1. Freeze WASM versions post–Tranche 2 arb stack (tag commit + WASM hashes).
2. Send RFP with this doc + repo link + mainnet IDs.
3. Ask for: threat model, CPI/auth review, economic attacks (slippage, caller abuse),
   fund-flow review, report + retest of fixes.
4. Record: engagement letter, final PDF, remediation tx hashes.

## Suggested vendors to quote

- Stellar-focused boutiques (search “Soroban smart contract audit”)
- SCF / Stellar Foundation partner lists when award is active
- Get **2 quotes** — one at ~$12k focused, one at ~$20k full — to validate $16k envelope

## If quotes exceed $16k

- Narrow scope: vault first (smaller, arb funds at risk), aggregator second tranche
- Or self-fund delta; do not reduce aggregator CPI review depth

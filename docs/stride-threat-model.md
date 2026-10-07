# LumAgg Soroban Contracts — STRIDE Threat Model

**Prepared:** 2026-10-04
**Scope:** Aggregator and Arb Vault
**Status:** Audit-intake attachment; final audit snapshot must record the frozen
Git commit and optimized WASM hashes.

## 1. System and trust boundaries

LumAgg's audited on-chain scope consists of the Aggregator and Arb Vault, plus
off-chain clients:

```text
User / integrator / keeper
        |
        v
User / integrator ----> Aggregator ----CPI----> DEX contracts
                              ^
                              |
                    Arb Vault ----authorized caller
                              |
                              +---- token balances
```

Trust boundaries:

1. User and integrator transactions are untrusted input.
2. The Arb Vault caller allowlist is a privileged boundary.
3. Aggregator administrator operations are a privileged boundary.
4. DEX contracts and token contracts are external dependencies; their return
   values, authorization behavior, and failure modes must not allow funds to be
   retained or bypass order constraints.
5. Off-chain quote and arbitrage services are untrusted callers. They
   may be unavailable or malicious, but must not be able to withdraw funds
   without on-chain authorization.

## 2. Protected assets

- User token balances involved in aggregator swaps.
- Arb Vault trading float and emergency-withdraw authority.
- Aggregator and Vault WASM upgrade authority.
- Atomicity and minimum-output guarantees.

## 3. What can go wrong — STRIDE issues

The issue identifiers below are unique so they can be tracked in the audit
report and remediation log. Each STRIDE category has at least one issue.

| Threat | Issue ID | Issue |
| --- | --- | --- |
| Spoofing | `Spoof.1` | An untrusted caller may present itself as an approved Vault caller or administrator. |
| Tampering | `Tamper.1` | A caller may alter route steps, token addresses, amounts, or minimum-output constraints. |
| Repudiation | `Repudiate.1` | A user, caller, or administrator may dispute which swap result or privileged action occurred. |
| Information Disclosure | `Info.1` | Public contract state and events may expose more balance or configuration information than intended. |
| Denial of Service | `DoS.1` | Hostile inputs, oversized routes, invalid DEX calls, or resource exhaustion may make swaps fail. |
| Elevation of Privilege | `Elevation.1` | A user-controlled address or external contract may attempt to gain admin, allowlist, withdrawal, upgrade, or token-spending authority. |

### Issue detail by trust boundary

| Threat | Attack surface | Existing mitigation / review focus |
| --- | --- | --- |
| Spoofing | Fake caller, unauthorized Vault caller, forged admin action | Soroban `require_auth`; Vault caller allowlist; admin-only initialization, upgrade, and withdrawal. Verify every privileged path and initialization race. |
| Tampering | User-supplied routes, token addresses, DEX IDs, amounts, and minimum outputs | Route validation, positive amount checks, connected-hop checks, minimum-output checks, and checked arithmetic. Verify no validation gap between public and restricted paths. |
| Repudiation | Ambiguous swaps, upgrades, or vault operations | Contract events for swap activity and privileged actions. Verify event fields are sufficient to reconstruct callers, amounts, and results. |
| Information disclosure | Public balances, admin, and caller configuration | Soroban contract state is public by design. Confirm no secret is expected on-chain and that off-chain keys/configuration are not embedded in contract state. |
| Denial of service | Oversized routes, invalid DEX CPI, repeated calls, resource exhaustion, hostile callbacks | Bounded route/step validation and atomic failure behavior. Review resource/footprint limits and griefing economics. |
| Elevation of privilege | Admin upgrade, Vault withdrawal, caller registration, nested token authorization | Admin authorization, allowlist checks, current-contract authorization, and restricted aggregator entry point. Verify no user-controlled address can become an authorized invoker or redirect funds. |

## 4. What are we going to do about it — treatments

| Issue | Treatment ID | Treatment / verification plan |
| --- | --- | --- |
| `Spoof.1` | `Spoof.1.R.1` | Require Soroban authorization for admin actions and enforce the Vault caller allowlist. |
| `Spoof.1` | `Spoof.1.R.2` | Retain negative tests for non-caller Vault execution, unauthorized admin actions, and forged nested authorization. |
| `Tamper.1` | `Tamper.1.R.1` | Validate positive amounts, token continuity, connected hops, route bounds, output-token consistency, split totals, and minimum-output floors before transfers or CPI. |
| `Tamper.1` | `Tamper.1.R.2` | Use checked arithmetic and explicit remainder handling for split and round-trip amounts; fuzz malformed routes and boundary values. |
| `Repudiate.1` | `Repudiate.1.R.1` | Emit and index swap, vault, and privileged-operation events with callers, tokens, amounts, and results. |
| `Repudiate.1` | `Repudiate.1.R.2` | Preserve the reviewed source commit, deployed WASM hashes, admin transactions, and remediation/retest evidence. |
| `Info.1` | `Info.1.R.1` | Treat Soroban storage and events as public; keep keys, RPC credentials, and quote-service credentials off-chain. |
| `Info.1` | `Info.1.R.2` | Review events, API output, and deployment artifacts for unnecessary personal data or operational secrets. |
| `DoS.1` | `DoS.1.R.1` | Bound route size, hop count, and resource growth; invalid DEX calls must fail atomically without consuming user funds. |
| `DoS.1` | `DoS.1.R.2` | Test repeated calls, zero/negative/overflow values, malformed venue calls, and testnet resource/footprint limits. |
| `Elevation.1` | `Elevation.1.R.1` | Restrict upgrade, caller allowlist, emergency withdrawal, and initialization to the intended admin/owner. |
| `Elevation.1` | `Elevation.1.R.2` | Verify token authorization is scoped to the intended vault flow and user-controlled addresses cannot redirect funds. |

## 5. Contract-specific security questions

### Aggregator

- Can a caller make the contract spend a token other than the declared input?
- Can split routes produce different output tokens or bypass the aggregate
  `min_amount_out` check?
- Can a malformed multi-hop route create a disconnected hop or redirect output
  to an unintended address?
- Are all DEX CPI calls constrained to the intended token flow and recipient?
- Does `round_trip_swap` rescale the return-leg weights without exceeding the
  actual bridge amount or weakening the final minimum-output guarantee?
- Are admin upgrade and venue-registry operations isolated from public swaps?

### Arb Vault

- Can an unlisted caller invoke `execute_round_trip` or move vault funds?
- Can the caller cause the vault to approve or reclaim more than the intended
  amount, or cause funds to remain with the caller after a failed route?
- Can a caller substitute an arbitrary Aggregator contract or token contract?
- Are deposit and emergency withdrawal authorization and amount checks correct?
- Are nested SAC authorization entries stable between simulation and submission?

## 6. Security tooling and test evidence

The repository includes unit and contract tests for route validation, split and
round-trip accounting, caller authorization, token flow, and restricted
execution paths. Order Escrow tests are retained in the repository but are not
evidence for this two-contract audit request.

On 2026-10-04, the following local test commands passed:

```text
cargo test --workspace --exclude aggregator-contract
cargo test -p aggregator-contract
```

The first command passed the workspace suites, including Vault (2 tests). The
second passed all 18 Aggregator contract tests.
Network-dependent tests that are intentionally ignored still require a separate
testnet/mainnet regression record before production deployment.

On 2026-10-08, CoinFabrik Scout Audit was run separately against both
in-scope contracts after the arithmetic and authorization-hardening changes:

| Contract | Critical | Medium | Minor | Enhancement |
| --- | ---: | ---: | ---: | ---: |
| Aggregator | 1 | 9 | 0 | 2 |
| Arb Vault | 2 | 4 | 0 | 3 |

The remaining Aggregator Critical finding is the upgrade-call detector, which
does not infer the preceding `require_admin` check. The Vault upgrade warning is
the same detector result. The Vault `transfer_from` finding is an intentional
part of the allowlisted-caller flow and requires manual auditor review of the
authorization and amount invariants. Route-size and hop-count bounds were not
added in this pass; the resulting unbounded-operation findings remain explicit
review items rather than being silently treated as fixed.

## 7. Did we do a good job?

- **Was the data-flow diagram referenced?** Yes. It identifies the user,
  authorized caller, vault, aggregator, token, and external DEX trust boundaries.
- **Did STRIDE identify new design concerns?** Yes. The model makes the Vault →
  Aggregator → token flow and the authorized-caller boundary explicit review
  targets.
- **Do treatments address the issues?** They define existing controls and
  required negative/property tests. The auditor must validate them and retest
  all critical/high remediation.
- **Were additional issues found after the model?** The model is living and must
  be updated after any architecture, authorization, DEX, or deployment change.

## 8. Deployment gates

- Freeze the exact source commit before audit.
- Record optimized WASM hashes for both in-scope contracts.
- Keep Order Escrow testnet-only; it requires a separate audit before any
  mainnet Limit/DCA deployment.
- Do not upgrade an existing mainnet contract solely to address an audit finding
  without recording the reviewed WASM, hash, admin authorization, and upgrade
  transaction.
- Re-run the relevant tests and obtain auditor retest approval for all critical
  and high findings.

## 9. Residual risks for auditor review

- Economic attacks and price manipulation are partly dependent on third-party
  DEX liquidity and route quality.
- External DEX contract upgrades or behavioral changes are outside LumAgg's
  direct control.
- Off-chain quote freshness, transaction timing, and keeper availability can
  affect execution but must not bypass on-chain authorization or minimum-output
  guarantees.
- The Audit Bank should confirm that its credits cover the two-contract scope.
- Order Escrow is intentionally excluded and must receive a separate audit
  commitment before mainnet deployment.

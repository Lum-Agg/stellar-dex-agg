# Arbitrage Diagnosis: 2026-10-26

## Scope

Mainnet observations from the production bot deployed at 08:35:30 UTC.
All probes below used unsigned `simulateTransaction` calls only. No token
transfer, TTL extension, or other transaction was broadcast during diagnosis.
Findings describe this observation window, not a proven start date for the
decline in trading activity.

## HITZ transfer failures

- Token: Gravity HITZ, `CBAPZAZNNB4X3VPXV2LYA5RMV7XHXIVREES2GG7R5GUXDZ4R4CKOY4EU`.
- XLM/HITZ Aquarius pool: `CCCDPF74BFBIHCBWCA3QX5R2UULH4VSJFOK6KL44KDKJS75ZKJJYUSPH`.
- HITZ/USDC Aquarius pool: `CBMOEJUOKI72AXRPEQCRYSWDUBMI2LZEYVFJUTB256FO3WYSYSZI5F5A`.
- Deployed HITZ WASM: `befa64d9d56feba32e18e09f1c364b8a4d863016337bb933ba12cc5c0a8e5b08`.

At the checked snapshot, diagnostics contained 2,901 preparation attempts:
1,769 structural failures, 1,132 fee rejections, and zero ready candidates.
Both HITZ pools appeared in all 1,769 structural-failure routes. These are
route-membership counts, not independent counts to add together.

A fixed XLM -> HITZ -> USDC -> XLM route reproduced the failure with a vault
at 100 XLM and without a vault at 1 XLM. The deepest reported error was in
HITZ `transfer`, when the XLM/HITZ pool tried to send HITZ to the Aggregator:

```text
HITZ.transfer(pool, Aggregator, amount)
VM call trapped: UnreachableCodeReached
HostError: Error(WasmVm, InvalidAction)
```

Direct pool swaps to an ordinary wallet, bypassing both LumAgg contracts,
also failed at HITZ `transfer` for 0.1 XLM and 1 XLM inputs. The token exposes
the standard `transfer(Address, Address, i128)` ABI, plus custom pool/router
registration and vault-status methods. LumAgg was not a registered router.
However, a direct token-transfer simulation to a registered router also
trapped, so a missing LumAgg router registration alone is not established as
the cause. Do not label this token malicious or infer its intended policy
from these observations alone.

Operational mitigation: exclude HITZ from quote paths, including hidden
intermediate hops, using `routing.excluded_tokens`. Do not disable all Aquarius
pools or merely remove HITZ from the bot's bridge-token list. This policy must
be enabled separately on every quote API instance; empty defaults change
nothing. See [configuration](aggregator-configuration.md#excluding-non-executable-tokens).

## Phoenix renewal costs

For the Phoenix XLM/USDC pool
`CBHCRSVX3ZZ7EGTSYMKPEFGZNWRVCSESQR3UABET4MIW52N4EVU6BIZX`,
unsigned simulations returned approximately 0.5702 XLM for `query_config`
and 0.5743 XLM for `query_pool_info`, even without performing a swap.

An isolated `ExtendFootprintTtl` simulation for the same instance and code
returned 5,853 stroops when no extension was needed, versus 5,693,072 stroops
when extending to 120,960 ledgers. The large quote was therefore dominated by
renewal, not ordinary swap computation. These are estimates, not fees paid.

Of the 12 Phoenix pools present in our Redis pool cache, 10 shared WASM
`167ab414a226427de34c19947ef9c5cf38c6c0ed91ecf9392f7cef3278ff506c`.
Its remaining TTL was about 96,500 ledgers, below the published six-day
renewal threshold. Query estimates for those pools were about 0.57-0.70 XLM;
one pool using different code had a query estimate of only 0.0007931 XLM.
This is a cache inventory, not a census of every Phoenix deployment.

The published Phoenix implementation calls `extend_ttl` from its query
methods. Its [TTL constants](https://github.com/Phoenix-Protocol-Group/phoenix-contracts/blob/main/packages/phoenix/src/ttl.rs)
use a six-day threshold and seven-day target. Public repository source is
supporting context, not a byte-for-byte verification of deployed code; the
isolated renewal simulation above is the direct on-chain-cost observation.

Shared code renewal should not be budgeted once per pool. Individual instance
renewal/restoration costs still need separate assessment. No paid maintenance
was performed, and these findings do not explain every historical failure.

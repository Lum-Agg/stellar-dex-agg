# Order Escrow Errors

The testnet Order Escrow contract exposes stable Soroban contract error codes
for validation failures. Clients should treat the numeric code as stable and
use the message only for display or logging.

Typical output is `Error(Contract, #<code>)`.

| Code | Name | Meaning |
| ---: | --- | --- |
| 1 | `NotInitialized` | The contract has not been initialized. |
| 2 | `AlreadyInitialized` | Initialization was already completed. |
| 3 | `InvalidAmount` | The input amount is zero or negative. |
| 4 | `InvalidLimit` | The limit price is zero or negative. |
| 5 | `SameToken` | Input and output tokens are identical. |
| 6 | `ExpirationInPast` | The expiration is not after the required start. |
| 7 | `ExpirationTooFar` | The expiration exceeds the maximum lifetime. |
| 8 | `OrderNotFound` | The requested order does not exist. |
| 9 | `OrderNotOpen` | The order is not open for the requested operation. |
| 10 | `OrderExpired` | The order has already expired. |
| 11 | `OrderNotExpired` | The order cannot yet be reclaimed. |
| 12 | `AmountExceedsRemaining` | The fill exceeds the remaining amount. |
| 13 | `InvalidRouteAmount` | Route amounts do not equal the requested input. |
| 14 | `MinimumOutBelowLimit` | The execution minimum is below the order limit. |
| 15 | `InvalidChunk` | The DCA chunk is zero or too large. |
| 16 | `InvalidInterval` | The DCA interval is zero. |
| 17 | `StartLedgerInPast` | The DCA start ledger is earlier than current. |
| 18 | `InvalidMinimumRate` | The DCA minimum output rate is negative. |
| 19 | `ChunkNotDue` | The next DCA chunk is not due yet. |
| 20 | `ArithmeticOverflow` | A checked arithmetic operation overflowed. |

## Current Testnet Deployment

```text
CCI3U3P7MPZNCA5L7KWTXNS7H7KV6AIZQ6ZY2FEOZPHTJIAVCRYPKXTM
```

An invalid DCA request with a past `start_ledger` returns
`Error(Contract, #17)` (`StartLedgerInPast`) instead of a generic trap.
This change applies to testnet only; the mainnet Aggregator was not upgraded.

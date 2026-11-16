# Order Escrow Errors

The testnet Order Escrow contract exposes stable Soroban contract error codes
for validation failures. Clients should treat the numeric code as stable and
use the message only for display or logging.

Typical output is `Error(Contract, #<code>)`.

| Code | Name | Meaning |
| ---: | --- | --- |
| 1 | `NotInitialized` | Required configuration or counters are missing. |
| 2 | Reserved | Former initialization error; not returned by new deployments. |
| 3 | `InvalidAmount` | The input amount is zero or negative. |
| 4 | `InvalidLimit` | The limit rate is non-positive or its required output is out of range. |
| 5 | `SameToken` | Input and output tokens are identical. |
| 6 | `ExpirationInPast` | The expiration is not after the required start. |
| 7 | `ExpirationTooFar` | The expiration exceeds the maximum lifetime. |
| 8 | `OrderNotFound` | The requested order does not exist. |
| 9 | `OrderNotOpen` | The order is not open for the requested operation. |
| 10 | `OrderExpired` | The order has already expired. |
| 11 | `OrderNotExpired` | The order cannot yet be reclaimed. |
| 12 | `AmountExceedsRemaining` | The fill exceeds the remaining amount. |
| 13 | `InvalidRouteAmount` | Route amounts do not equal the requested input. |
| 14 | `MinimumOutBelowLimit` | The requested minimum is below the order floor or settlement is below the requested minimum. |
| 15 | `InvalidChunk` | The DCA chunk is zero or too large. |
| 16 | `InvalidInterval` | The DCA interval is zero. |
| 17 | `StartLedgerInPast` | The DCA start ledger is earlier than current. |
| 18 | `InvalidMinimumRate` | The DCA rate is non-positive or its required chunk output is out of range. |
| 19 | `ChunkNotDue` | The next DCA chunk is not due yet. |
| 20 | `ArithmeticOverflow` | A checked arithmetic operation overflowed. |
| 21 | `UnexpectedTokenBalance` | A deposit, swap, or payout did not have the expected balance changes. |
| 22 | `AssetNotAllowed` | The input or output asset is not approved for creation or fills. |
| 23 | `InsufficientOrderTtl` | The network TTL limit cannot cover expiry and the refund grace period. |
| 24 | `ScheduleExceedsExpiry` | The earliest possible final DCA chunk is not before expiry. |

Missing or invalid keeper authorization is a Soroban authorization error,
not an Order Escrow contract error code.

## Current Testnet Deployment

```text
CCI3U3P7MPZNCA5L7KWTXNS7H7KV6AIZQ6ZY2FEOZPHTJIAVCRYPKXTM
```

An invalid DCA request with a past `start_ledger` returns
`Error(Contract, #17)` (`StartLedgerInPast`) instead of a generic trap.
This change applies to testnet only; the mainnet Aggregator was not upgraded.

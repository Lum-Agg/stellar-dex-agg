# User swaps and arbitrage reporting

The `/stats` dashboard headlines the transaction split for the full indexed
period. Counts include successful indexed Aggregator invocations only:

- **User swaps:** `by_function.swap`, including split routes executed by `swap`.
- **Arbitrage:** `by_function.round_trip_swap`, even when a return amount is
  missing and the transaction cannot be included in gross-surplus calculations.
- **Other / unclassified:** successful invocations minus those two categories.

Percentages use all successful indexed invocations as the denominator, not
attempts, failed transactions, DEX legs, or only transactions with known profits.
No percentage is reported when the denominator is zero.

"User swaps" describes a contract function, not verified independent users:
these calls may include team activity or bots. Unique caller addresses across
all functions include arbitrage callers and must not be presented as user adoption.

Existing entry-notional and actual DEX-leg volume totals remain combined metrics,
including arbitrage. They are not user-only volume. Multi-hop and round-trip
execution can process the same principal through multiple legs.

The stats JSON already provides the successful `by_function` counts. CSV schema
v2 includes `user_swap_count`, `arbitrage_count`, `other_success_count`,
`user_swap_share_pct`, and `arbitrage_share_pct`. The older JSON `round_trip_count`
(CSV `round_trips_with_return_amount`) remains the count with complete return
amounts used for surplus reporting; use `arbitrage_count` for the activity split.

## CSV schema v2 and USD amounts

This is a deliberate CSV schema change. Consumers should use column headers,
not fixed column positions. The JSON field names are retained for compatibility.

- `csv_schema_version` is `2`.
- `unique_caller_addresses` includes arbitrage callers, not only swap users.
- The invalid mixed-token `notional_in_stroops` and `routed_dex_volume_stroops`
  columns are removed. `entry_amounts_by_token_raw` and
  `routed_amounts_by_token_raw` contain CSV-escaped JSON arrays with token IDs and
  exact raw amounts encoded as decimal strings. Raw units are token-specific,
  not USD or XLM. XLM and USDC SAC amounts use seven decimals.
- USD columns are explicitly labelled `priced_subtotal`. Only known XLM
  daily-reference prices and the assumed USDC/USD peg of 1 are used. Unknown
  tokens or missing XLM prices are excluded, never silently treated as XLM.
- Routed USD uses only event-derived leg inputs with known token identities and
  available prices. No multiplication of aggregate entry amounts by average hop
  counts, and no two-times-principal fallback, is used.
- `observed_dex_leg_count` counts all indexed legs; `actual_amount_leg_count`
  counts legs with actual inputs; `routed_priced_leg_count` counts the subset
  with a token identity and available USD price. Coverage is the last count
  divided by the first. It is blank if no legs were observed, and zero when
  observed legs exist but none can be priced.
- An unavailable USD subtotal is blank, not zero. XLM pricing is a daily
  reference, not a transaction-time rate or guaranteed UTC close. The current
  UTC day is provisional.

Old exports cannot be repaired from the CSV alone: they lack per-token inputs
and successful function counts. Regenerate from the indexed records after API
deployment. Do not infer missing actual amounts or function classifications.
Any previously corrupted indexed records require a separate source-data repair;
this export change does not reconstruct historical on-chain events.

This change does not alter smart contracts, swap execution, or arbitrage strategy.
The website and API CSV changes require deployment before they are public.

## Chart presentation

The stats explorer uses Recharts with shared LumAgg-themed containers, legends,
and tooltips. Transaction composition, USD volume, execution outcomes, caller
addresses, and gross surplus are separate charts; there is no USD/count dual axis.

All explorer charts use the same selected UTC range, ending at the newest indexed
day. Missing calendar dates and unavailable prices remain null gaps. Known zero
values are retained. Lines use linear interpolation and do not connect null gaps.
Headline totals and attribution panels outside the explorer cover the full indexed
period, not the selected chart range.

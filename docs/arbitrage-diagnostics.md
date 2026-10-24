# Arbitrage Simulation Diagnostics

The bot records quote-versus-simulation differences for candidates that reach
transaction preparation. These diagnostics are observational: they do not
disable pools, change position sizing, lower profit floors, or add RPC calls.

## Enable persistent diagnostics

Keep the submission database on persistent, writable storage:

```toml
[execution]
submission_db_path = "/var/lib/lumagg/arb-submissions.db"
```

The same database contains transaction reconciliation and daily simulation
aggregates. Existing databases are migrated automatically. Without this
setting, only bounded in-memory aggregates and session counters are available.
Data starts accumulating after this version is deployed; historical logs and
past transactions are not backfilled.

## Read the report

```bash
./lumagg-arbitrage-bot --diagnostics-db /var/lib/lumagg/arb-submissions.db
```

This standalone command opens the database read-only, reads a consistent
snapshot, prints JSON, and exits. It does not load caller secrets, connect to
RPC, start the scanner, or submit transactions. Do not combine it with
`--config` or `--check-config`. The database must already exist and contain the
new diagnostics schema.

The report covers the current UTC day and the preceding six UTC days, with the
20 busiest groups separately for `pairs`, `venues`, and `pools`. It also shows
the earliest/latest retained day and current retained row count. Capacity
limits can shorten coverage or remove part of a day; this is not a promise of
complete historical coverage.

## Interpret the fields

- `base`, `bridge`, and `amount_bucket` keep different token pairs and trade
  sizes separate. Buckets use raw base-token units: `[0,1e7)`, `[1e7,1e8)`,
  `[1e8,1e9)`, `[1e9,1e10)`, `[1e10,1e11)`, and `[1e11,infinity)`. For a
  seven-decimal token these correspond to below 1, 1-10, 10-100, 100-1,000,
  1,000-10,000, and at least 10,000 tokens. They are not USD notionals.
- `attempts` counts preparation attempts, including each probe-size retry.
  `ready`, `fee_rejected`, `economic_failures`, `structural_failures`, and
  `prepare_errors` are mutually exclusive outcomes. `ready` means preparation
  passed the net-profit gate, not that a trade was broadcast or confirmed.
- `output_samples` counts attempts with an output value, including recovered
  base-token output from failed simulation diagnostics. Failed attempts with
  no recoverable output do not contribute a zero gap or enter the gap mean.
- `avg_gap_bps`, `min_gap_bps`, and `max_gap_bps` compare quoted output to
  simulated output relative to input principal. Positive means the quote was
  optimistic. Values are clipped to +/-1,000,000 bps as an arithmetic safety
  bound. `optimistic_samples` counts samples with a positive rounded bps gap.
- `max_reported_pool_age_ms` is the age reported by the quote API, not a
  guarantee of snapshot alignment with the simulation ledger. `missing_pool_age`
  counts attempts where either leg lacks age metadata. `max_simulation_ms`
  measures the preparation call, including RPC simulation and assembly.
- Venue and pool rows describe **route membership, not fault attribution**.
  A route's full return comparison is attributed to each participating venue
  and pool once, even when it repeats across hops/splits. Do not sum those
  membership counts as distinct attempts. CLMM and non-CLMM venue types stay
  separate. Use the existing hop-comparison probe to prove which hop diverges.

Quotes rejected before transaction preparation, scan-only mode, and quote API
failures are not sampled here. Read the existing quote/bridge funnel alongside
this report; a successful simulation does not prove a local quote was accurate
before any execution-time quote validation or route refresh.

## Inspect a daily trend

Use `scope = 'pair'` for non-duplicated route-level totals:

```sql
SELECT datetime(day, 'unixepoch') AS utc_day, base, bridge, amount_bucket,
       attempts, ready, fee_rejected, economic_failures,
       structural_failures, prepare_errors,
       1.0 * gap_bps_sum / NULLIF(output_samples, 0) AS avg_gap_bps
FROM simulation_metrics
WHERE scope = 'pair'
ORDER BY day DESC, attempts DESC;
```

## Resource and failure limits

The trading path only updates memory. A background blocking worker runs every
five seconds and writes at most eight batches of 128 dimension aggregates per
pass, stopping between batches after one second. SQLite's configured busy
timeout can extend a blocked batch. Writes share the submission database, so
storage contention may delay other database operations; use local reliable
storage and monitor flush warnings.

The pending map is limited to 4,096 dimensions and drained round-robin. Database
transactions roll back the entire batch on failure; pending aggregates are
restored within the memory cap. Crashes can lose unflushed diagnostic samples.
The five-minute stats log includes `diagnostic_dropped_dimensions` (a session
count of dimension observations lost to capacity). These metrics are
best-effort diagnostics, not an accounting ledger.

Each successful batch flush prunes rows outside the last 30 UTC calendar days,
with a hard cap of 100,000 aggregate rows; the oldest diagnostic rows are removed first. Pruning
never deletes submission/accounting records. SQLite can reuse freed pages
without shrinking the file. Use SQLite's online backup API or `.backup` for a
consistent backup while the bot is running.

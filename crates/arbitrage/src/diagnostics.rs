//! Bounded, best-effort simulation diagnostics. Never used for trade decisions.

use crate::{
    bridge::RoundTripQuote,
    runtime::SharedRuntime,
    submission_ledger::{unix_seconds, SubmissionLedger},
};
use anyhow::Result;
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

const MAX_PENDING: usize = 4096;
const BATCH_SIZE: usize = 128;
const MAX_ROWS: usize = 100_000;
const DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy)]
pub enum SimOutcome {
    Ready,
    FeeRejected,
    EconomicFailure,
    StructuralFailure,
    PrepareError,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct Dimension {
    day: i64,
    base: String,
    bridge: String,
    amount_bucket: String,
    scope: String,
    identity: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Aggregate {
    attempts: u64,
    ready: u64,
    fee_rejected: u64,
    economic_failures: u64,
    structural_failures: u64,
    prepare_errors: u64,
    output_samples: u64,
    optimistic_samples: u64,
    gap_bps_sum: i64,
    gap_bps_min: Option<i64>,
    gap_bps_max: Option<i64>,
    missing_pool_age: u64,
    max_pool_age_ms: Option<u64>,
    max_simulation_ms: u64,
    last_seen: i64,
}

impl Aggregate {
    fn merge(&mut self, other: &Self) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.ready = self.ready.saturating_add(other.ready);
        self.fee_rejected = self.fee_rejected.saturating_add(other.fee_rejected);
        self.economic_failures = self.economic_failures.saturating_add(other.economic_failures);
        self.structural_failures = self.structural_failures.saturating_add(other.structural_failures);
        self.prepare_errors = self.prepare_errors.saturating_add(other.prepare_errors);
        self.output_samples = self.output_samples.saturating_add(other.output_samples);
        self.optimistic_samples = self.optimistic_samples.saturating_add(other.optimistic_samples);
        self.gap_bps_sum = self.gap_bps_sum.saturating_add(other.gap_bps_sum);
        self.gap_bps_min = self.gap_bps_min.into_iter().chain(other.gap_bps_min).min();
        self.gap_bps_max = self.gap_bps_max.into_iter().chain(other.gap_bps_max).max();
        self.missing_pool_age = self.missing_pool_age.saturating_add(other.missing_pool_age);
        self.max_pool_age_ms = self.max_pool_age_ms.into_iter().chain(other.max_pool_age_ms).max();
        self.max_simulation_ms = self.max_simulation_ms.max(other.max_simulation_ms);
        self.last_seen = self.last_seen.max(other.last_seen);
    }
}

#[derive(Debug, Default)]
pub struct SimulationBook {
    pending: Mutex<BTreeMap<Dimension, Aggregate>>,
    cursor: Mutex<Option<Dimension>>,
    pub dropped_dimensions: AtomicU64,
}

impl SimulationBook {
    pub fn record(&self, quote: &RoundTripQuote, outcome: SimOutcome, output: Option<u128>, elapsed_ms: u64) {
        let now = unix_seconds();
        let mut row = Aggregate {
            attempts: 1,
            max_simulation_ms: elapsed_ms.min(i64::MAX as u64),
            last_seen: now,
            missing_pool_age: u64::from(
                quote.leg_out.pool_state_age_ms.is_none() || quote.leg_back.pool_state_age_ms.is_none(),
            ),
            max_pool_age_ms: quote.pool_state_age_ms().map(|n| n.min(i64::MAX as u64)),
            ..Default::default()
        };
        match outcome {
            SimOutcome::Ready => row.ready = 1,
            SimOutcome::FeeRejected => row.fee_rejected = 1,
            SimOutcome::EconomicFailure => row.economic_failures = 1,
            SimOutcome::StructuralFailure => row.structural_failures = 1,
            SimOutcome::PrepareError => row.prepare_errors = 1,
        }
        if let Some(output) = output {
            // Bound pathological numeric inputs so SQL sums stay integral.
            let gap =
                crate::stats::quote_sim_gap_bps(quote.amount_in, quote.amount_out, output).clamp(-1_000_000, 1_000_000);
            row.output_samples = 1;
            row.optimistic_samples = u64::from(gap > 0);
            row.gap_bps_sum = gap;
            row.gap_bps_min = Some(gap);
            row.gap_bps_max = Some(gap);
        }
        // A pool repeated across hops/splits contributes once per simulation.
        let mut identities = BTreeSet::from([("pair".to_owned(), "all".to_owned())]);
        for step in quote
            .leg_out
            .step_sets
            .iter()
            .chain(&quote.leg_back.step_sets)
            .flatten()
        {
            let venue = if step.venue_type.is_empty() {
                &step.dex_type
            } else {
                &step.venue_type
            };
            identities.insert(("venue".into(), venue.clone()));
            identities.insert(("pool".into(), format!("{venue}:{}", step.pool_address)));
        }
        let mut pending = self.pending.lock().expect("simulation diagnostics mutex poisoned");
        for (scope, identity) in identities {
            let key = Dimension {
                day: now / DAY * DAY,
                base: quote.base.canonical(),
                bridge: quote.bridge.canonical(),
                amount_bucket: amount_bucket(quote.amount_in).into(),
                scope,
                identity,
            };
            if let Some(existing) = pending.get_mut(&key) {
                existing.merge(&row);
            } else if pending.len() < MAX_PENDING {
                pending.insert(key, row.clone());
            } else {
                self.dropped_dimensions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn take_batch(&self) -> Vec<(Dimension, Aggregate)> {
        let mut pending = self.pending.lock().expect("simulation diagnostics mutex poisoned");
        let mut cursor = self.cursor.lock().expect("simulation diagnostics cursor poisoned");
        let mut keys: Vec<_> = if let Some(last) = cursor.as_ref() {
            use std::ops::Bound::{Excluded, Unbounded};
            pending
                .range((Excluded(last), Unbounded))
                .take(BATCH_SIZE)
                .map(|(key, _)| key.clone())
                .collect()
        } else {
            pending.keys().take(BATCH_SIZE).cloned().collect()
        };
        if let Some(last) = cursor.as_ref() {
            let remaining = BATCH_SIZE - keys.len();
            keys.extend(
                pending
                    .range(..=last.clone())
                    .take(remaining)
                    .map(|(key, _)| key.clone()),
            );
        }
        if let Some(last) = keys.last() {
            *cursor = Some(last.clone());
        }
        keys.into_iter()
            .map(|key| {
                let row = pending.remove(&key).unwrap();
                (key, row)
            })
            .collect()
    }

    fn restore(&self, batch: Vec<(Dimension, Aggregate)>) {
        let mut pending = self.pending.lock().expect("simulation diagnostics mutex poisoned");
        for (key, row) in batch {
            if let Some(existing) = pending.get_mut(&key) {
                existing.merge(&row);
            } else if pending.len() < MAX_PENDING {
                pending.insert(key, row);
            } else {
                self.dropped_dimensions.fetch_add(row.attempts, Ordering::Relaxed);
            }
        }
    }
}

fn amount_bucket(amount: u128) -> &'static str {
    match amount {
        0..10_000_000 => "[0,1e7)",
        10_000_000..100_000_000 => "[1e7,1e8)",
        100_000_000..1_000_000_000 => "[1e8,1e9)",
        1_000_000_000..10_000_000_000 => "[1e9,1e10)",
        10_000_000_000..100_000_000_000 => "[1e10,1e11)",
        _ => "[1e11,infinity)",
    }
}

pub(crate) fn initialize(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS simulation_metrics (
        day INTEGER NOT NULL, base TEXT NOT NULL, bridge TEXT NOT NULL, amount_bucket TEXT NOT NULL,
        scope TEXT NOT NULL, identity TEXT NOT NULL,
        attempts INTEGER NOT NULL, ready INTEGER NOT NULL, fee_rejected INTEGER NOT NULL,
        economic_failures INTEGER NOT NULL, structural_failures INTEGER NOT NULL, prepare_errors INTEGER NOT NULL,
        output_samples INTEGER NOT NULL, optimistic_samples INTEGER NOT NULL, gap_bps_sum INTEGER NOT NULL,
        gap_bps_min INTEGER, gap_bps_max INTEGER, missing_pool_age INTEGER NOT NULL,
        max_pool_age_ms INTEGER, max_simulation_ms INTEGER NOT NULL, last_seen INTEGER NOT NULL,
        PRIMARY KEY(day, base, bridge, amount_bucket, scope, identity));
        CREATE INDEX IF NOT EXISTS simulation_metrics_retention ON simulation_metrics(day, last_seen);",
    )?;
    Ok(())
}

pub(crate) fn persist(
    connection: &mut Connection,
    rows: &[(Dimension, Aggregate)],
    now: i64,
    max_rows: usize,
) -> Result<()> {
    let transaction = connection.transaction()?;
    {
        let mut stmt = transaction.prepare_cached("INSERT INTO simulation_metrics VALUES
            (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
            ON CONFLICT(day,base,bridge,amount_bucket,scope,identity) DO UPDATE SET
            attempts=attempts+excluded.attempts, ready=ready+excluded.ready, fee_rejected=fee_rejected+excluded.fee_rejected,
            economic_failures=economic_failures+excluded.economic_failures,
            structural_failures=structural_failures+excluded.structural_failures, prepare_errors=prepare_errors+excluded.prepare_errors,
            output_samples=output_samples+excluded.output_samples, optimistic_samples=optimistic_samples+excluded.optimistic_samples,
            gap_bps_sum=gap_bps_sum+excluded.gap_bps_sum,
            gap_bps_min=CASE WHEN gap_bps_min IS NULL THEN excluded.gap_bps_min WHEN excluded.gap_bps_min IS NULL THEN gap_bps_min ELSE MIN(gap_bps_min,excluded.gap_bps_min) END,
            gap_bps_max=CASE WHEN gap_bps_max IS NULL THEN excluded.gap_bps_max WHEN excluded.gap_bps_max IS NULL THEN gap_bps_max ELSE MAX(gap_bps_max,excluded.gap_bps_max) END,
            missing_pool_age=missing_pool_age+excluded.missing_pool_age,
            max_pool_age_ms=CASE WHEN max_pool_age_ms IS NULL THEN excluded.max_pool_age_ms WHEN excluded.max_pool_age_ms IS NULL THEN max_pool_age_ms ELSE MAX(max_pool_age_ms,excluded.max_pool_age_ms) END,
            max_simulation_ms=MAX(max_simulation_ms,excluded.max_simulation_ms), last_seen=MAX(last_seen,excluded.last_seen)")?;
        for (key, row) in rows {
            stmt.execute(params![
                key.day,
                key.base,
                key.bridge,
                key.amount_bucket,
                key.scope,
                key.identity,
                row.attempts,
                row.ready,
                row.fee_rejected,
                row.economic_failures,
                row.structural_failures,
                row.prepare_errors,
                row.output_samples,
                row.optimistic_samples,
                row.gap_bps_sum,
                row.gap_bps_min,
                row.gap_bps_max,
                row.missing_pool_age,
                row.max_pool_age_ms,
                row.max_simulation_ms,
                row.last_seen
            ])?;
        }
    }
    transaction.execute(
        "DELETE FROM simulation_metrics WHERE day < ?1",
        [now / DAY * DAY - 29 * DAY],
    )?;
    transaction.execute(
        "DELETE FROM simulation_metrics WHERE rowid IN
        (SELECT rowid FROM simulation_metrics ORDER BY day,last_seen LIMIT
        MAX(0,(SELECT COUNT(*) FROM simulation_metrics)-?1))",
        [max_rows as i64],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn spawn_persistence(runtime: SharedRuntime) {
    let Some(ledger) = runtime.submission_ledger.clone() else {
        return;
    };
    let book = runtime.stats.simulations.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let book = book.clone();
            let ledger = ledger.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || flush(&book, &ledger)).await {
                tracing::warn!(%error, "simulation diagnostics writer stopped unexpectedly");
            }
        }
    });
}

fn flush(book: &SimulationBook, ledger: &SubmissionLedger) {
    let started = std::time::Instant::now();
    for _ in 0..8 {
        let batch = book.take_batch();
        if batch.is_empty() {
            break;
        }
        if let Err(error) = ledger.persist_simulation_metrics(&batch, unix_seconds(), MAX_ROWS) {
            book.restore(batch);
            tracing::warn!(%error, "simulation diagnostics flush failed; retaining bounded pending aggregates");
            break;
        }
        if started.elapsed() >= Duration::from_secs(1) {
            break;
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DiagnosticRow {
    base: String,
    bridge: String,
    amount_bucket: String,
    identity: String,
    attempts: u64,
    ready: u64,
    fee_rejected: u64,
    economic_failures: u64,
    structural_failures: u64,
    prepare_errors: u64,
    output_samples: u64,
    optimistic_samples: u64,
    avg_gap_bps: Option<f64>,
    min_gap_bps: Option<i64>,
    max_gap_bps: Option<i64>,
    missing_pool_age: u64,
    max_reported_pool_age_ms: Option<u64>,
    max_simulation_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    since_unix: i64,
    earliest_retained_day: Option<i64>,
    latest_retained_day: Option<i64>,
    retained_rows: u64,
    notes: [&'static str; 3],
    pairs: Vec<DiagnosticRow>,
    venues: Vec<DiagnosticRow>,
    pools: Vec<DiagnosticRow>,
}

/// No schema creation, runtime initialization, credentials, or RPC requests.
pub fn read_report(path: &Path) -> Result<DiagnosticReport> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(2))?;
    let since = unix_seconds() / DAY * DAY - 6 * DAY;
    let transaction = connection.unchecked_transaction()?;
    let (earliest_retained_day, latest_retained_day, retained_rows) =
        transaction.query_row("SELECT MIN(day), MAX(day), COUNT(*) FROM simulation_metrics", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    let report = DiagnosticReport { since_unix: since, earliest_retained_day, latest_retained_day, retained_rows,
        notes: ["Current UTC day and preceding six days; top 20 groups per scope by attempt count. Retention caps may shorten coverage.",
            "Venue/pool rows describe route membership, not hop-level fault attribution; do not sum memberships as distinct attempts.",
            "Only prepared candidates are sampled; outputs may be recovered from failed simulation diagnostics. Positive gap means optimistic quote; gaps capped at +/-1000000 bps. Amount buckets use raw base units."],
        pairs: read_scope(&transaction, since, "pair")?, venues: read_scope(&transaction, since, "venue")?,
        pools: read_scope(&transaction, since, "pool")?, };
    transaction.commit()?;
    Ok(report)
}

fn read_scope(connection: &Connection, since: i64, scope: &str) -> Result<Vec<DiagnosticRow>> {
    let mut stmt = connection.prepare("SELECT base,bridge,amount_bucket,identity,
        SUM(attempts),SUM(ready),SUM(fee_rejected),SUM(economic_failures),SUM(structural_failures),SUM(prepare_errors),
        SUM(output_samples),SUM(optimistic_samples),1.0*SUM(gap_bps_sum)/NULLIF(SUM(output_samples),0),
        MIN(gap_bps_min),MAX(gap_bps_max),SUM(missing_pool_age),MAX(max_pool_age_ms),MAX(max_simulation_ms)
        FROM simulation_metrics WHERE day >= ?1 AND scope = ?2
        GROUP BY base,bridge,amount_bucket,identity ORDER BY SUM(attempts) DESC,base,bridge,amount_bucket,identity LIMIT 20")?;
    let rows = stmt.query_map(params![since, scope], |r| {
        Ok(DiagnosticRow {
            base: r.get(0)?,
            bridge: r.get(1)?,
            amount_bucket: r.get(2)?,
            identity: r.get(3)?,
            attempts: r.get(4)?,
            ready: r.get(5)?,
            fee_rejected: r.get(6)?,
            economic_failures: r.get(7)?,
            structural_failures: r.get(8)?,
            prepare_errors: r.get(9)?,
            output_samples: r.get(10)?,
            optimistic_samples: r.get(11)?,
            avg_gap_bps: r.get(12)?,
            min_gap_bps: r.get(13)?,
            max_gap_bps: r.get(14)?,
            missing_pool_age: r.get(15)?,
            max_reported_pool_age_ms: r.get(16)?,
            max_simulation_ms: r.get(17)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{invoke::ArbSwapStep, quote_client::LegQuote};
    use router_engine::{OptimalRoute, TokenId};

    fn quote(amount: u128) -> RoundTripQuote {
        let leg = LegQuote {
            route: OptimalRoute {
                sub_orders: vec![],
                total_amount_in: amount,
                total_expected_out: amount,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                minimum_out: 0,
                compute_time_ms: 0,
                debug: None,
            },
            step_sets: vec![vec![
                ArbSwapStep {
                    venue_type: "aquarius_clmm".into(),
                    dex_type: "aquarius".into(),
                    pool_address: "CLMM_POOL".into(),
                    token_in: "A".into(),
                    token_out: "B".into(),
                    in_idx: 0,
                    out_idx: 1,
                },
                ArbSwapStep {
                    venue_type: "aquarius".into(),
                    dex_type: "aquarius".into(),
                    pool_address: "XYK_POOL".into(),
                    token_in: "B".into(),
                    token_out: "A".into(),
                    in_idx: 0,
                    out_idx: 1,
                },
            ]],
            minimum_out: 0,
            snapshot_age_ms: Some(50),
            pool_state_age_ms: Some(25),
        };
        RoundTripQuote {
            base: TokenId::from_str_auto("XLM"),
            bridge: TokenId::from_str_auto("USDC"),
            amount_in: amount,
            amount_out: amount + 1_000_000,
            minimum_out: 0,
            leg_out: leg.clone(),
            leg_back: leg,
        }
    }

    fn sample_book() -> SimulationBook {
        let book = SimulationBook::default();
        let quote = quote(100_000_000);
        book.record(&quote, SimOutcome::EconomicFailure, Some(99_000_000), 10);
        book.record(&quote, SimOutcome::Ready, Some(100_500_000), 20);
        let mut missing_age = quote;
        missing_age.leg_out.pool_state_age_ms = None;
        book.record(&missing_age, SimOutcome::StructuralFailure, None, 30);
        book
    }

    fn temporary_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lumagg-diag-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn all_outcomes_and_repeated_pool_membership_are_counted_once() {
        let book = sample_book();
        let rows = book.take_batch();
        assert_eq!(rows.len(), 5); // pair, two distinct venues, two distinct pools
        for (_, row) in &rows {
            assert_eq!(row.attempts, 3);
            assert_eq!(row.ready, 1);
            assert_eq!(row.economic_failures, 1);
            assert_eq!(row.structural_failures, 1);
            assert_eq!(row.output_samples, 2);
            assert_eq!(row.gap_bps_sum, 250);
            assert_eq!(row.gap_bps_min, Some(50));
            assert_eq!(row.gap_bps_max, Some(200));
            assert_eq!(row.missing_pool_age, 1);
            assert_eq!(row.max_simulation_ms, 30);
        }
        assert!(rows.iter().any(|(k, _)| k.identity == "aquarius_clmm:CLMM_POOL"));
        let quote = quote(1);
        book.record(&quote, SimOutcome::FeeRejected, Some(1), 1);
        book.record(&quote, SimOutcome::PrepareError, None, 1);
        let rows = book.take_batch();
        assert!(rows.iter().all(|(_, r)| r.fee_rejected == 1 && r.prepare_errors == 1));
    }

    #[test]
    fn raw_amount_buckets_have_exact_integer_boundaries() {
        assert_eq!(amount_bucket(9_999_999), "[0,1e7)");
        assert_eq!(amount_bucket(10_000_000), "[1e7,1e8)");
        assert_eq!(amount_bucket(100_000_000), "[1e8,1e9)");
        assert_eq!(amount_bucket(100_000_000_000), "[1e11,infinity)");
        assert_eq!(amount_bucket(u128::MAX), "[1e11,infinity)");
    }

    #[test]
    fn write_failure_rolls_back_and_restores_aggregates_without_double_counting() {
        let book = sample_book();
        let batch = book.take_batch();
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        db.execute_batch(
            "CREATE TRIGGER reject_metrics BEFORE INSERT ON simulation_metrics
            WHEN NEW.scope='venue' BEGIN SELECT RAISE(ABORT,'test write failure'); END;",
        )
        .unwrap();
        assert!(persist(&mut db, &batch, unix_seconds(), MAX_ROWS).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM simulation_metrics", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            0
        );
        book.restore(batch);
        db.execute_batch("DROP TRIGGER reject_metrics").unwrap();
        persist(&mut db, &book.take_batch(), unix_seconds(), MAX_ROWS).unwrap();
        let rows = read_scope(&db, 0, "pair").unwrap();
        assert_eq!(rows[0].attempts, 3);
        assert_eq!(rows[0].avg_gap_bps, Some(125.0));
        // A second independent batch is added, not substituted.
        persist(&mut db, &sample_book().take_batch(), unix_seconds(), MAX_ROWS).unwrap();
        assert_eq!(read_scope(&db, 0, "pair").unwrap()[0].attempts, 6);
    }

    #[test]
    fn persisted_report_survives_reopen_and_read_only_missing_path_does_not_create() {
        let path = temporary_path("reopen");
        let ledger = SubmissionLedger::open(&path).unwrap();
        flush(&sample_book(), &ledger);
        drop(ledger);
        let report = read_report(&path).unwrap();
        assert_eq!(report.pairs[0].attempts, 3);
        assert_eq!(report.pairs[0].avg_gap_bps, Some(125.0));
        assert_eq!(report.pools.len(), 2);
        assert!(serde_json::to_string(&report).unwrap().contains("route membership"));
        let _ = std::fs::remove_file(path);
        let missing = temporary_path("missing");
        assert!(read_report(&missing).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn retention_and_row_cap_prune_only_diagnostics() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        db.execute_batch("CREATE TABLE submissions (hash TEXT); INSERT INTO submissions VALUES ('keep');")
            .unwrap();
        let now = unix_seconds();
        let mut rows = sample_book().take_batch();
        rows[0].0.day = now / DAY * DAY - 30 * DAY;
        persist(&mut db, &rows, now, 2).unwrap();
        let count: u64 = db
            .query_row("SELECT COUNT(*) FROM simulation_metrics", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let expired: u64 = db
            .query_row(
                "SELECT COUNT(*) FROM simulation_metrics WHERE day < ?1",
                [now / DAY * DAY - 29 * DAY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(expired, 0);
        assert_eq!(
            db.query_row("SELECT hash FROM submissions", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
    }

    #[test]
    fn pending_capacity_and_round_robin_flushing_are_bounded() {
        let book = SimulationBook::default();
        let template = sample_book().take_batch().remove(0);
        for i in 0..MAX_PENDING {
            let mut key = template.0.clone();
            key.identity = format!("{i:05}");
            book.pending.lock().unwrap().insert(key, template.1.clone());
        }
        book.record(&quote(100_000_000), SimOutcome::Ready, Some(101_000_000), 1);
        assert_eq!(book.pending.lock().unwrap().len(), MAX_PENDING);
        assert_eq!(book.dropped_dimensions.load(Ordering::Relaxed), 5);
        let first = book.take_batch();
        assert_eq!(first.len(), BATCH_SIZE);
        book.restore(first.clone());
        let next = book.take_batch();
        assert!(next.iter().all(|(key, _)| !first.iter().any(|(old, _)| key == old)));
    }

    #[test]
    fn missing_outputs_are_null_not_zero_gaps_and_currencies_stay_separate() {
        let book = SimulationBook::default();
        let xlm = quote(100_000_000);
        let mut usdc = xlm.clone();
        usdc.base = TokenId::from_str_auto("USDC");
        book.record(&xlm, SimOutcome::PrepareError, None, 1);
        book.record(&usdc, SimOutcome::Ready, Some(101_000_000), 2);
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        persist(&mut db, &book.take_batch(), unix_seconds(), MAX_ROWS).unwrap();
        let rows = read_scope(&db, 0, "pair").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .any(|r| r.base == xlm.base.canonical() && r.avg_gap_bps.is_none() && r.output_samples == 0));
        assert!(rows
            .iter()
            .any(|r| r.base == usdc.base.canonical() && r.avg_gap_bps == Some(0.0) && r.output_samples == 1));
    }
}

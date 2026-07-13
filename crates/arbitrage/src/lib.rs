//! Soroban round-trip arbitrage via LumAgg aggregator `round_trip_swap`.

pub mod bridge;
pub mod callers;
pub mod config;
pub mod context;
pub mod dedup;
pub mod execute;
pub mod hydrate;
pub mod invoke;
pub mod keypair;
pub mod optimize;
pub mod prepare;
pub mod runtime;
pub mod scanner;
pub mod stats;
pub mod submit;
pub mod vault;

pub use {
    bridge::RoundTripQuote,
    config::ArbConfig,
    context::ArbContext,
    execute::{execution_enabled, PreparedArbTx},
    invoke::{path_to_steps, ArbSwapStep},
    runtime::{ArbRuntime, SharedRuntime},
    scanner::{collect_paths_for_base, scan_once, ArbOpportunity},
    stats::{ArbStats, ArbStatsSnapshot},
};

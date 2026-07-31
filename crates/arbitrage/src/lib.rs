//! Soroban round-trip arbitrage via LumAgg aggregator `round_trip_swap`.

pub mod bridge;
pub mod callers;
pub mod config;
pub mod context;
pub mod dedup;
pub mod economics;
pub mod execute;
pub mod invoke;
pub mod keypair;
pub mod optimize;
pub mod pipeline;
pub mod prepare;
pub mod probe;
pub mod profit;
pub mod quote_client;
pub mod runtime;
pub mod scanner;
pub mod stats;
pub mod submit;
pub mod telegram;
pub mod vault;

pub use {
    bridge::RoundTripQuote,
    config::ArbConfig,
    context::ArbContext,
    execute::{execution_enabled, PreparedArbTx},
    invoke::{path_to_steps, ArbSwapStep},
    pipeline::engine::start_bot,
    quote_client::{LegQuote, QuoteApiClient},
    runtime::{ArbRuntime, SharedRuntime},
    scanner::{evaluate_bridge_pair, scan_once, ArbOpportunity},
    stats::{ArbStats, ArbStatsSnapshot, QuietWindowAlert, QuietWindowTracker},
};

//! DEX adapter trait and implementations for Stellar ecosystem DEXes.
//!
//! # Architecture Note: Classic DEX vs Soroban DEX
//!
//! Stellar's native Classic DEX (PathPayment) has **uncontrollable routing**:
//! when you submit a PathPayment, Stellar Core automatically finds the best
//! execution across orderbooks + liquidity pools. You cannot force it to use
//! a specific pool or path.
//!
//! This means Classic DEX is NOT a controllable liquidity source for aggregation.
//! Instead, our aggregator focuses on **Soroban DEXes** (Aquarius, Soroswap,
//! Phoenix, Comet) where each swap is a deterministic contract call with
//! predictable output.
//!
//! Classic DEX serves as:
//! - A **benchmark** to compare against ("is our Soroban route better than PathPayment?")
//! - A **fallback** for tokens only available on the native orderbook
//!
//! The core value proposition: aggregate liquidity across isolated Soroban DEX
//! contracts that Stellar Core's native routing cannot reach.

pub mod aquarius;
pub mod aquarius_clmm;
pub mod batch_refresh;
pub mod cache;
pub mod classic_dex;
pub mod clmm_math;
pub mod comet;
pub mod comet_math;
pub mod phoenix;
pub mod rpc;
pub mod soroswap;
pub mod stable_math;
pub mod sushi;
pub mod token_metadata;
pub mod token_registry;
pub mod traits;
pub mod utils;

pub use cache::{default_cache_path, PoolCache};
pub use rpc::SorobanRpc;
pub use sushi::SushiAdapter;
pub use token_registry::TokenRegistry;
pub use traits::*;

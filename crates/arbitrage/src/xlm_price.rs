//! Live XLM→USDC mark for converting Soroban XLM fees into USDC base units.

use {
    crate::{
        economics::{UNIT_E7, USDC_SAC, XLM_SAC},
        quote_client::QuoteApiClient,
    },
    anyhow::{anyhow, Result},
    std::sync::atomic::{AtomicU64, Ordering},
    tracing::{info, warn},
};

/// Reject marks outside this band (USDC units per 1.0 XLM, 7 decimals).
const MIN_PRICE_E7: u128 = 500_000; // $0.05
const MAX_PRICE_E7: u128 = 10_000_000; // $1.00

/// Shared XLM/USDC price used by USDC-base fee gates.
#[derive(Debug)]
pub struct XlmUsdcPrice {
    price_e7: AtomicU64,
    fallback_e7: u64,
}

impl XlmUsdcPrice {
    pub fn new(fallback_e7: u128) -> Self {
        let fallback = clamp_u64(fallback_e7.max(1));
        Self {
            price_e7: AtomicU64::new(fallback),
            fallback_e7: fallback,
        }
    }

    pub fn get(&self) -> u128 {
        self.price_e7.load(Ordering::Relaxed) as u128
    }

    pub fn fallback(&self) -> u128 {
        self.fallback_e7 as u128
    }

    /// Quote 1.0 XLM → USDC and update the cached mark.
    pub async fn refresh(&self, client: &QuoteApiClient) -> Result<u128> {
        let out = client.quote_expected_output(XLM_SAC, USDC_SAC, UNIT_E7).await?;
        if !(MIN_PRICE_E7..=MAX_PRICE_E7).contains(&out) {
            warn!(
                quoted_e7 = out,
                previous_e7 = self.get(),
                "XLM/USDC quote outside sanity band — keeping previous mark"
            );
            return Err(anyhow!("XLM/USDC mark {out} outside [{MIN_PRICE_E7}, {MAX_PRICE_E7}]"));
        }
        let prev = self.get();
        self.price_e7.store(clamp_u64(out), Ordering::Relaxed);
        if prev != out {
            info!(
                xlm_usdc_price_e7 = out,
                previous_e7 = prev,
                fallback_e7 = self.fallback_e7,
                "refreshed XLM/USDC mark for USDC fee gates"
            );
        }
        Ok(out)
    }
}

fn clamp_u64(v: u128) -> u64 {
    u64::try_from(v.min(u64::MAX as u128)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_fallback() {
        let p = XlmUsdcPrice::new(1_800_000);
        assert_eq!(p.get(), 1_800_000);
        assert_eq!(p.fallback(), 1_800_000);
    }
}

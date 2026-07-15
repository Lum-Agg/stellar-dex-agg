//! Discrete input scan to maximize round-trip profit (via quote-api).

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        context::ArbContext,
    },
    router_engine::TokenId,
};

/// Logarithmically spaced candidate inputs between `min_in` and `max_in`
/// (inclusive endpoints). Dense at small sizes where historical arb profit
/// concentrates; sparse near the ceiling.
pub fn build_candidate_inputs(min_in: u128, max_in: u128, sample_count: usize) -> Vec<u128> {
    if sample_count == 0 || min_in == 0 || max_in == 0 || min_in > max_in {
        return Vec::new();
    }
    if sample_count == 1 || min_in == max_in {
        return vec![min_in];
    }

    let mut out = Vec::with_capacity(sample_count);
    let log_min = (min_in as f64).ln();
    let log_max = (max_in as f64).ln();
    let den = (sample_count - 1) as f64;
    for i in 0..sample_count {
        let t = i as f64 / den;
        let v = (log_min + (log_max - log_min) * t).exp().round() as u128;
        let v = v.clamp(min_in, max_in);
        if v > 0 {
            out.push(v);
        }
    }
    // Keep endpoints exact after float round-trip.
    if let Some(first) = out.first_mut() {
        *first = min_in;
    }
    if let Some(last) = out.last_mut() {
        *last = max_in;
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Pick the input with the highest absolute profit for a base/bridge pair.
pub async fn optimize_round_trip(
    ctx: &ArbContext,
    base: &TokenId,
    bridge: &TokenId,
    min_in: u128,
    max_in: u128,
    sample_count: usize,
) -> Option<RoundTripQuote> {
    let candidates = build_candidate_inputs(min_in, max_in, sample_count);
    let mut best: Option<RoundTripQuote> = None;

    for amount_in in candidates {
        let Ok(quote) = quote_round_trip(ctx, base, bridge, amount_in).await else {
            continue;
        };
        let profit = quote.profit();
        match &best {
            None => best = Some(quote),
            Some(b) if profit > b.profit() => best = Some(quote),
            _ => {}
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::build_candidate_inputs;

    #[test]
    fn candidate_inputs_cover_range() {
        let v = build_candidate_inputs(100, 1000, 5);
        assert_eq!(v.first(), Some(&100));
        assert_eq!(v.last(), Some(&1000));
        assert!(v.len() >= 2);
    }

    #[test]
    fn candidate_inputs_are_log_spaced() {
        // 10 → 500 XLM, 10 samples: denser below ~200 than linear would be.
        let min_in = 100_000_000u128;
        let max_in = 5_000_000_000u128;
        let v = build_candidate_inputs(min_in, max_in, 10);
        assert_eq!(v.first(), Some(&min_in));
        assert_eq!(v.last(), Some(&max_in));
        assert_eq!(v.len(), 10);
        // Midpoint by index should be well below linear midpoint (255 XLM).
        let mid = v[4] as f64 / 1e7;
        assert!(mid < 100.0, "expected log-mid < 100 XLM, got {mid}");
        assert!(mid > 40.0, "expected log-mid > 40 XLM, got {mid}");
    }
}

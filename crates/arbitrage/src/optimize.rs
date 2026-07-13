//! Discrete input scan to maximize round-trip profit (via quote-api).

use {
    crate::{
        bridge::{quote_round_trip, RoundTripQuote},
        context::ArbContext,
    },
    router_engine::TokenId,
};

/// Evenly spaced candidate inputs between `min_in` and `max_in` (inclusive
/// endpoints).
pub fn build_candidate_inputs(min_in: u128, max_in: u128, sample_count: usize) -> Vec<u128> {
    if sample_count == 0 || min_in == 0 || max_in == 0 || min_in > max_in {
        return Vec::new();
    }
    if sample_count == 1 {
        return vec![min_in];
    }
    let mut out = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        let num = i as u128;
        let den = (sample_count - 1) as u128;
        let v = min_in + (max_in - min_in) * num / den;
        if v > 0 {
            out.push(v);
        }
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
}

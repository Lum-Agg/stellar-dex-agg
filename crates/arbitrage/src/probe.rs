//! Offline quote-vs-chain comparison helpers (no arb hot path).

use crate::scanner::compute_profit_bps;

#[derive(Debug, Clone)]
pub struct HopCompare {
    pub index: usize,
    pub source: String,
    pub pool: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_out: Option<u128>,
}

/// (local_out − chain_out) / amount_in in bps, using local as the reference
/// notional for the hop input. Positive ⇒ local optimistic vs chain.
pub fn hop_gap_bps(amount_in: u128, local_out: u128, chain_out: u128) -> i64 {
    let local_bps = compute_profit_bps(amount_in, local_out);
    let chain_bps = compute_profit_bps(amount_in, chain_out);
    local_bps.saturating_sub(chain_bps)
}

pub fn first_diverging_hop(hops: &[HopCompare], threshold_bps: i64) -> Option<usize> {
    for h in hops {
        let Some(chain) = h.chain_out else {
            return Some(h.index);
        };
        if h.local_out == 0 {
            continue;
        }
        if hop_gap_bps(h.amount_in, h.local_out, chain).abs() >= threshold_bps {
            return Some(h.index);
        }
    }
    None
}

/// Deterministic sampling with a simple LCG seeded RNG (no extra deps).
pub fn pick_bridges(bridges: &[String], count: usize, seed: u64) -> Vec<String> {
    if bridges.is_empty() || count == 0 {
        return Vec::new();
    }
    let mut state = seed.max(1);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let idx = (state as usize) % bridges.len();
        out.push(bridges[idx].clone());
    }
    out
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeSampleReport {
    pub mode: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_path_out: Option<u128>,
    pub gap_bps: Option<i64>,
    pub first_bad_hop: Option<usize>,
    pub hops: Vec<HopCompareReport>,
    pub simulate_out: Option<u128>,
    pub simulate_gap_bps: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoundTripProbeReport {
    pub bridge: String,
    pub amount_in: u128,
    pub quoted_out: u128,
    pub quoted_profit_bps: i64,
    pub leg_out: ProbeSampleReport,
    pub leg_back: ProbeSampleReport,
    pub simulate_out: Option<u128>,
    pub simulate_gap_bps: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HopCompareReport {
    pub index: usize,
    pub source: String,
    pub pool: String,
    pub amount_in: u128,
    pub local_out: u128,
    pub chain_out: Option<u128>,
    pub gap_bps: Option<i64>,
}

/// Absolute gap (bps) for median exit-code checks: prefer simulate gap, else max
/// of both leg path gaps when available.
pub fn round_trip_abs_gap_bps(report: &RoundTripProbeReport) -> Option<u64> {
    if let Some(gap) = report.simulate_gap_bps {
        return Some(gap.unsigned_abs());
    }
    match (report.leg_out.gap_bps, report.leg_back.gap_bps) {
        (Some(out), Some(back)) => Some(out.unsigned_abs().max(back.unsigned_abs())),
        _ => None,
    }
}

impl From<&HopCompare> for HopCompareReport {
    fn from(h: &HopCompare) -> Self {
        let gap_bps = match (h.local_out, h.chain_out) {
            (local, Some(chain)) if local > 0 => Some(hop_gap_bps(h.amount_in, local, chain)),
            _ => None,
        };
        Self {
            index: h.index,
            source: h.source.clone(),
            pool: h.pool.clone(),
            amount_in: h.amount_in,
            local_out: h.local_out,
            chain_out: h.chain_out,
            gap_bps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_bps_matches_20bps_fixture() {
        let amount_in = 100_000_000u128;
        let local = 100_143_095u128;
        let chain = 99_942_226u128;
        let gap = hop_gap_bps(amount_in, local, chain);
        assert!((19..=21).contains(&gap), "gap={gap}");
    }

    #[test]
    fn first_diverging_hop_picks_earliest_above_threshold() {
        let hops = vec![
            HopCompare {
                index: 0,
                source: "soroswap".into(),
                pool: "P0".into(),
                amount_in: 100_000_000,
                local_out: 50_000_000,
                chain_out: Some(50_000_000),
            },
            HopCompare {
                index: 1,
                source: "aquarius".into(),
                pool: "P1".into(),
                amount_in: 50_000_000,
                local_out: 100_200_000,
                chain_out: Some(99_900_000),
            },
        ];
        let idx = first_diverging_hop(&hops, 5).expect("should find hop 1");
        assert_eq!(idx, 1);
    }

    #[test]
    fn sample_round_robin_bridges_is_deterministic_with_seed() {
        let bridges = vec!["A".into(), "B".into(), "C".into()];
        let a = pick_bridges(&bridges, 5, 42);
        let b = pick_bridges(&bridges, 5, 42);
        assert_eq!(a, b);
        assert_eq!(a.len(), 5);
    }

    #[test]
    fn first_diverging_hop_skips_unknown_local_out_on_intermediate_hops() {
        let hops = vec![
            HopCompare {
                index: 0,
                source: "soroswap".into(),
                pool: "P0".into(),
                amount_in: 100_000_000,
                local_out: 0,
                chain_out: Some(50_000_000),
            },
            HopCompare {
                index: 1,
                source: "aquarius".into(),
                pool: "P1".into(),
                amount_in: 50_000_000,
                local_out: 100_200_000,
                chain_out: Some(99_900_000),
            },
        ];
        let idx = first_diverging_hop(&hops, 5).expect("should find hop 1");
        assert_eq!(idx, 1);
    }

    #[test]
    fn first_diverging_hop_returns_chain_failure_not_unknown_local() {
        let hops = vec![
            HopCompare {
                index: 0,
                source: "soroswap".into(),
                pool: "P0".into(),
                amount_in: 100_000_000,
                local_out: 0,
                chain_out: Some(50_000_000),
            },
            HopCompare {
                index: 1,
                source: "aquarius".into(),
                pool: "P1".into(),
                amount_in: 50_000_000,
                local_out: 0,
                chain_out: None,
            },
        ];
        let idx = first_diverging_hop(&hops, 5).expect("should find hop 1");
        assert_eq!(idx, 1);
    }

    #[test]
    fn hop_compare_report_omits_gap_when_local_out_unknown() {
        let hop = HopCompare {
            index: 0,
            source: "soroswap".into(),
            pool: "P0".into(),
            amount_in: 100_000_000,
            local_out: 0,
            chain_out: Some(50_000_000),
        };
        let report = HopCompareReport::from(&hop);
        assert_eq!(report.gap_bps, None);
    }

    fn sample_leg(gap_bps: Option<i64>) -> ProbeSampleReport {
        ProbeSampleReport {
            mode: "one-leg".into(),
            token_in: "BASE".into(),
            token_out: "BRIDGE".into(),
            amount_in: 100,
            local_out: 101,
            chain_path_out: Some(100),
            gap_bps,
            first_bad_hop: None,
            hops: vec![],
            simulate_out: None,
            simulate_gap_bps: None,
            error: None,
        }
    }

    fn round_trip_report(
        leg_out_gap: Option<i64>,
        leg_back_gap: Option<i64>,
        simulate_gap_bps: Option<i64>,
    ) -> RoundTripProbeReport {
        RoundTripProbeReport {
            bridge: "BRIDGE".into(),
            amount_in: 100,
            quoted_out: 102,
            quoted_profit_bps: 200,
            leg_out: sample_leg(leg_out_gap),
            leg_back: sample_leg(leg_back_gap),
            simulate_out: None,
            simulate_gap_bps,
            error: None,
        }
    }

    #[test]
    fn round_trip_abs_gap_prefers_simulate_gap() {
        let report = round_trip_report(Some(5), Some(20), Some(-15));
        assert_eq!(round_trip_abs_gap_bps(&report), Some(15));
    }

    #[test]
    fn round_trip_abs_gap_uses_max_leg_gap_without_simulate() {
        let report = round_trip_report(Some(5), Some(20), None);
        assert_eq!(round_trip_abs_gap_bps(&report), Some(20));
    }

    #[test]
    fn round_trip_abs_gap_none_when_legs_incomplete() {
        let report = round_trip_report(Some(5), None, None);
        assert_eq!(round_trip_abs_gap_bps(&report), None);
    }

    #[test]
    fn round_trip_report_serializes_both_legs() {
        let leg = ProbeSampleReport {
            mode: "one-leg".into(),
            token_in: "BASE".into(),
            token_out: "BRIDGE".into(),
            amount_in: 100,
            local_out: 101,
            chain_path_out: Some(100),
            gap_bps: Some(100),
            first_bad_hop: None,
            hops: vec![],
            simulate_out: None,
            simulate_gap_bps: None,
            error: None,
        };
        let report = RoundTripProbeReport {
            bridge: "BRIDGE".into(),
            amount_in: 100,
            quoted_out: 102,
            quoted_profit_bps: 200,
            leg_out: leg.clone(),
            leg_back: leg,
            simulate_out: None,
            simulate_gap_bps: None,
            error: None,
        };

        let value = serde_json::to_value(report).expect("report serializes");
        assert!(value.get("leg_out").is_some());
        assert!(value.get("leg_back").is_some());
    }
}

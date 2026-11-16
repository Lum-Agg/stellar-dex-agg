//! Per-base profit floors and fee conversion (Soroban fees are paid in XLM).

/// Native XLM SAC (mainnet).
pub const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
/// Circle USDC SAC (mainnet).
pub const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

/// 1.0 token in 7-decimal base units.
pub const UNIT_E7: u128 = 10_000_000;

/// Convert a Soroban resource fee (XLM stroops) into base-token units for the
/// profit gate.
///
/// - XLM base: fee stays in stroops.
/// - USDC base: `fee_xlm * xlm_usdc_price_e7 / 1e7` (price = USDC units per 1
///   XLM).
/// - Other bases: treat fee as XLM stroops (conservative / legacy).
pub fn fee_in_base_units(fee_xlm_stroops: u128, base_token: &str, xlm_usdc_price_e7: u128) -> u128 {
    if base_token == XLM_SAC || base_token.is_empty() {
        return fee_xlm_stroops;
    }
    if base_token == USDC_SAC {
        if xlm_usdc_price_e7 == 0 {
            return fee_xlm_stroops;
        }
        // Round up so we never understate gas in USDC terms.
        return fee_xlm_stroops
            .saturating_mul(xlm_usdc_price_e7)
            .saturating_add(UNIT_E7 - 1) /
            UNIT_E7;
    }
    fee_xlm_stroops
}

/// Resolve min profit floor for a base token (base-token units, 7 decimals).
pub fn min_profit_for_base(
    base_token: &str,
    default_min_profit: u128,
    min_profit_xlm: Option<u128>,
    min_profit_usdc: Option<u128>,
) -> u128 {
    if base_token == XLM_SAC {
        return min_profit_xlm.unwrap_or(default_min_profit);
    }
    if base_token == USDC_SAC {
        return min_profit_usdc.unwrap_or(default_min_profit);
    }
    default_min_profit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlm_fee_unchanged() {
        assert_eq!(fee_in_base_units(115_155, XLM_SAC, 3_000_000), 115_155);
    }

    #[test]
    fn usdc_fee_scales_with_xlm_price() {
        // 0.0115155 XLM * 0.30 USDC/XLM ≈ 0.00345465 USDC → 34547 (ceil)
        assert_eq!(fee_in_base_units(115_155, USDC_SAC, 3_000_000), 34_547);
    }

    #[test]
    fn usdc_fee_zero_price_falls_back_to_raw() {
        assert_eq!(fee_in_base_units(115_155, USDC_SAC, 0), 115_155);
    }

    #[test]
    fn observed_usdc_opp_passes_with_converted_fee() {
        let sim_profit = 92_342u128;
        let fee_base = fee_in_base_units(115_155, USDC_SAC, 3_000_000);
        let net = sim_profit.saturating_sub(fee_base);
        let min = min_profit_for_base(USDC_SAC, 80_000, None, Some(30_000));
        assert!(fee_base < 40_000, "fee_base={fee_base}");
        assert!(net >= min, "net={net} min={min} fee={fee_base}");
    }

    #[test]
    fn min_profit_overrides() {
        assert_eq!(min_profit_for_base(XLM_SAC, 80_000, Some(90_000), Some(30_000)), 90_000);
        assert_eq!(
            min_profit_for_base(USDC_SAC, 80_000, Some(90_000), Some(30_000)),
            30_000
        );
        assert_eq!(min_profit_for_base(USDC_SAC, 80_000, None, None), 80_000);
        assert_eq!(min_profit_for_base("COTHER", 80_000, Some(1), Some(2)), 80_000);
    }
}

//! Quote-engine backed USDC price marks.

use {
    crate::state::AppState,
    router_engine::{RouteRequest, TokenId},
};

pub const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
pub const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
pub const TOKEN_UNITS: u128 = 10_000_000;

/// Returns a token's USDC mark and the route denomination used to obtain it.
pub async fn mark_token_usdc(state: &AppState, token: &str) -> Option<(f64, &'static str)> {
    if token == USDC_SAC {
        return Some((1.0, "usdc"));
    }

    if let Some(price) = quote_price(state, token, USDC_SAC).await {
        return Some((price, "usdc"));
    }

    let token_xlm = quote_price(state, token, XLM_SAC).await?;
    let xlm_usdc = quote_price(state, XLM_SAC, USDC_SAC).await?;
    Some((token_xlm * xlm_usdc, "xlm"))
}

async fn quote_price(state: &AppState, token_in: &str, token_out: &str) -> Option<f64> {
    let route = state
        .quote_route(&RouteRequest {
            token_in: TokenId::from_str_auto(token_in),
            token_out: TokenId::from_str_auto(token_out),
            amount_in: TOKEN_UNITS,
            slippage_bps: Some(50),
            max_hops: None,
            max_splits: None,
            prefer_soroban: None,
        })
        .await;

    (!route.sub_orders.is_empty())
        .then(|| route.total_expected_out as f64 / TOKEN_UNITS as f64)
        .filter(|price| price.is_finite() && *price > 0.0)
}

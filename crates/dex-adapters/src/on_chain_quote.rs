//! On-chain hop quotes for validating local AMM / CLMM math.
//!
//! Aquarius (classic + CLMM) pools expose `estimate_swap(in_idx, out_idx,
//! in_amount)`. Soroswap pairs use fresh `get_reserves()` + the same xy=k
//! formula as the adapter.

use {
    crate::{
        rpc::{scval_to_address, scval_to_u128, SorobanRpc},
        soroswap::SoroswapAdapter,
    },
    anyhow::Result,
    stellar_xdr::curr as xdr,
    tracing::debug,
};

pub fn amount_u128_scval(amount: u128) -> xdr::ScVal {
    xdr::ScVal::U128(xdr::UInt128Parts {
        hi: (amount >> 64) as u64,
        lo: amount as u64,
    })
}

/// Pool `estimate_swap(in_idx, out_idx, in_amount)` — Aquarius classic + CLMM.
pub async fn estimate_swap(
    rpc: &SorobanRpc,
    pool_address: &str,
    in_idx: u32,
    out_idx: u32,
    amount_in: u128,
) -> Result<Option<u128>> {
    if amount_in == 0 {
        return Ok(Some(0));
    }
    let args = vec![
        xdr::ScVal::U32(in_idx),
        xdr::ScVal::U32(out_idx),
        amount_u128_scval(amount_in),
    ];
    match rpc.simulate_call(pool_address, "estimate_swap", args).await {
        Ok(result) => Ok(scval_to_u128(&result).ok()),
        Err(e) => {
            debug!(
                pool = pool_address,
                in_idx,
                out_idx,
                amount_in,
                error = %e,
                "estimate_swap failed"
            );
            Ok(None)
        }
    }
}

/// Fresh-reserve Soroswap xy=k output (matches on-chain pair math).
pub async fn soroswap_amount_out(
    rpc: &SorobanRpc,
    pair_address: &str,
    token_in: &str,
    amount_in: u128,
) -> Result<Option<u128>> {
    if amount_in == 0 {
        return Ok(Some(0));
    }

    let (token_0, token_1, reserves) = tokio::try_join!(
        rpc.call_no_args(pair_address, "token_0"),
        rpc.call_no_args(pair_address, "token_1"),
        rpc.call_no_args(pair_address, "get_reserves"),
    )?;

    let token_0 = scval_to_address(&token_0)?;
    let token_1 = scval_to_address(&token_1)?;

    let (reserve_in, reserve_out) = if token_in == token_0 {
        parse_soroswap_reserves(&reserves, true)?
    } else if token_in == token_1 {
        parse_soroswap_reserves(&reserves, false)?
    } else {
        return Ok(None);
    };

    let out = SoroswapAdapter::compute_output(amount_in, reserve_in, reserve_out);
    Ok(if out > 0 { Some(out) } else { None })
}

fn parse_soroswap_reserves(val: &xdr::ScVal, token0_is_in: bool) -> Result<(u128, u128)> {
    let xdr::ScVal::Vec(Some(vec)) = val else {
        anyhow::bail!("get_reserves: expected vec");
    };
    if vec.0.len() < 2 {
        anyhow::bail!("get_reserves: expected at least 2 elements");
    }
    let r0 = scval_to_u128(&vec.0[0])?;
    let r1 = scval_to_u128(&vec.0[1])?;
    Ok(if token0_is_in { (r0, r1) } else { (r1, r0) })
}

/// Best-effort on-chain output for one Soroban hop.
pub async fn hop_amount_out_on_chain(
    rpc: &SorobanRpc,
    source: &str,
    pool_address: &str,
    token_in: &str,
    _token_out: &str,
    in_idx: u32,
    out_idx: u32,
    amount_in: u128,
) -> Result<Option<u128>> {
    match source {
        "aquarius" | "aquarius_clmm" => estimate_swap(rpc, pool_address, in_idx, out_idx, amount_in).await,
        "soroswap" => soroswap_amount_out(rpc, pool_address, token_in, amount_in).await,
        _ => Ok(None),
    }
}

/// Walk a multi-hop path on-chain; returns final output or None if any hop
/// fails.
pub async fn path_amount_out_on_chain(
    rpc: &SorobanRpc,
    sources: &[String],
    pool_addresses: &[String],
    tokens: &[String],
    in_indices: &[u32],
    out_indices: &[u32],
    amount_in: u128,
) -> Result<Option<u128>> {
    let hops = sources.len();
    if hops == 0
        || pool_addresses.len() != hops
        || tokens.len() != hops + 1
        || in_indices.len() != hops
        || out_indices.len() != hops
    {
        return Ok(None);
    }

    let mut current = amount_in;
    for i in 0..hops {
        let out = hop_amount_out_on_chain(
            rpc,
            &sources[i],
            &pool_addresses[i],
            &tokens[i],
            &tokens[i + 1],
            in_indices[i],
            out_indices[i],
            current,
        )
        .await?;
        current = match out {
            Some(v) if v > 0 => v,
            _ => return Ok(None),
        };
    }
    Ok(Some(current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_u128_scval_roundtrip() {
        let amount = 100_000_000u128;
        let val = amount_u128_scval(amount);
        assert_eq!(scval_to_u128(&val).unwrap(), amount);
    }
}

//! Parse DEX router contract events to recover touched pool addresses.
//!
//! Pool contracts emit their own events (preferred path in
//! [`super::pool_index`]). When users route through Aquarius / Soroswap
//! routers, the router also emits events whose `contractId` is the router —
//! pool id lives in the event body.

use {
    crate::{
        aquarius::AQUARIUS_ROUTER,
        rpc::{get_map_field, scval_to_address},
        soroswap::SOROSWAP_ROUTER,
        utils::is_contract_address,
    },
    base64::Engine,
    stellar_xdr::curr::{Limits, ReadXdr, ScVal},
};

const AQUARIUS_POOL_OPS: &[&str] = &["deposit", "swap", "withdraw", "add_pool"];
const SOROSWAP_LIQUIDITY_OPS: &[&str] = &["add", "remove"];

/// Extract known pool contract addresses referenced by a router event.
pub fn pools_from_router_event(contract_id: &str, topics: Option<&[String]>, value: Option<&str>) -> Vec<String> {
    let Some(topics) = topics else {
        return Vec::new();
    };
    if topics.is_empty() {
        return Vec::new();
    }

    if contract_id == AQUARIUS_ROUTER {
        return pools_from_aquarius_router(topics, value);
    }
    if contract_id == SOROSWAP_ROUTER {
        return pools_from_soroswap_router(topics, value);
    }
    Vec::new()
}

fn pools_from_aquarius_router(topics: &[String], value: Option<&str>) -> Vec<String> {
    let Some(op) = topic_symbol(topics, 0) else {
        return Vec::new();
    };
    if !AQUARIUS_POOL_OPS.contains(&op.as_str()) {
        return Vec::new();
    }
    let Some(body) = value.and_then(decode_scval_b64) else {
        return Vec::new();
    };
    // Body: (pool_id, ...) for deposit/swap/withdraw; (pool_address, ...) for
    // add_pool.
    first_contract_address(&body).into_iter().collect()
}

fn pools_from_soroswap_router(topics: &[String], value: Option<&str>) -> Vec<String> {
    // Topics: ("SoroswapRouter", "add"|"remove"|"swap").
    let Some(op) = topic_symbol(topics, 1) else {
        return Vec::new();
    };
    if !SOROSWAP_LIQUIDITY_OPS.contains(&op.as_str()) {
        // Swap path lists tokens, not pair addresses — rely on pair contract events.
        return Vec::new();
    }
    let Some(body) = value.and_then(decode_scval_b64) else {
        return Vec::new();
    };
    pair_from_soroswap_liquidity_body(&body).into_iter().collect()
}

fn pair_from_soroswap_liquidity_body(body: &ScVal) -> Option<String> {
    if let ScVal::Map(Some(map)) = body {
        for key in ["pair"] {
            if let Some(val) = get_map_field(map, key) {
                if let Some(addr) = contract_address(val) {
                    return Some(addr);
                }
            }
        }
    }
    // Fallback: first contract address in the struct body.
    first_contract_address(body)
}

fn topic_symbol(topics: &[String], index: usize) -> Option<String> {
    let raw = topics.get(index)?;
    let scval = decode_scval_b64(raw)?;
    scval_symbol(&scval)
}

fn decode_scval_b64(raw: &str) -> Option<ScVal> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
    ScVal::from_xdr(&bytes, Limits::none()).ok()
}

fn scval_symbol(val: &ScVal) -> Option<String> {
    match val {
        ScVal::Symbol(s) => Some(s.to_string()),
        ScVal::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn contract_address(val: &ScVal) -> Option<String> {
    scval_to_address(val).ok().filter(|addr| is_contract_address(addr))
}

fn first_contract_address(val: &ScVal) -> Option<String> {
    match val {
        ScVal::Address(_) => contract_address(val),
        ScVal::Vec(Some(vec)) => vec.0.iter().find_map(first_contract_address),
        ScVal::Map(Some(map)) => map.0.iter().find_map(|entry| first_contract_address(&entry.val)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        stellar_strkey::Contract,
        stellar_xdr::curr::{ScAddress, WriteXdr},
    };

    fn addr(seed: u8) -> ScVal {
        let hash = [seed; 32];
        ScVal::Address(ScAddress::Contract(stellar_xdr::curr::ContractId(
            stellar_xdr::curr::Hash(hash),
        )))
    }

    fn pool_str(seed: u8) -> String {
        format!("{}", Contract([seed; 32]))
    }

    fn b64(val: &ScVal) -> String {
        base64::engine::general_purpose::STANDARD.encode(val.to_xdr(Limits::none()).expect("xdr"))
    }

    #[test]
    fn aquarius_router_swap_body_yields_pool_id() {
        let pool = addr(7);
        let body = ScVal::Vec(Some(stellar_xdr::curr::ScVec(
            vec![pool, ScVal::U32(1)].try_into().unwrap(),
        )));
        let topics = vec![b64(&ScVal::Symbol(
            stellar_xdr::curr::ScSymbol::try_from("swap").unwrap(),
        ))];
        let pools = pools_from_router_event(AQUARIUS_ROUTER, Some(&topics), Some(&b64(&body)));
        assert_eq!(pools, vec![pool_str(7)]);
    }

    #[test]
    fn soroswap_router_add_yields_pair_field() {
        let pair = addr(9);
        let map = stellar_xdr::curr::ScMap(
            vec![stellar_xdr::curr::ScMapEntry {
                key: ScVal::Symbol(stellar_xdr::curr::ScSymbol::try_from("pair").unwrap()),
                val: pair,
            }]
            .try_into()
            .unwrap(),
        );
        let body = ScVal::Map(Some(map));
        let topics = vec![
            b64(&ScVal::Symbol(
                stellar_xdr::curr::ScSymbol::try_from("SoroswapRouter").unwrap(),
            )),
            b64(&ScVal::Symbol(stellar_xdr::curr::ScSymbol::try_from("add").unwrap())),
        ];
        let pools = pools_from_router_event(SOROSWAP_ROUTER, Some(&topics), Some(&b64(&body)));
        assert_eq!(pools, vec![pool_str(9)]);
    }

    #[test]
    fn soroswap_router_swap_is_ignored() {
        let body = ScVal::Vec(Some(stellar_xdr::curr::ScVec(vec![ScVal::U32(1)].try_into().unwrap())));
        let topics = vec![
            b64(&ScVal::Symbol(
                stellar_xdr::curr::ScSymbol::try_from("SoroswapRouter").unwrap(),
            )),
            b64(&ScVal::Symbol(stellar_xdr::curr::ScSymbol::try_from("swap").unwrap())),
        ];
        let pools = pools_from_router_event(SOROSWAP_ROUTER, Some(&topics), Some(&b64(&body)));
        assert!(pools.is_empty());
    }
}

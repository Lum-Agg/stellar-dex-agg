//! Classify DEX contract event topics (swap / add LP / remove LP / …).

use {
    base64::Engine,
    stellar_xdr::curr::{Limits, ReadXdr, ScVal},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DexEventKind {
    Swap,
    AddLiquidity,
    RemoveLiquidity,
    StateUpdate,
    PoolDiscovery,
    RewardsAdmin,
    Other,
}

impl DexEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Swap => "swap",
            Self::AddLiquidity => "add_lp",
            Self::RemoveLiquidity => "remove_lp",
            Self::StateUpdate => "state_update",
            Self::PoolDiscovery => "pool_discovery",
            Self::RewardsAdmin => "rewards_admin",
            Self::Other => "other",
        }
    }
}

/// Primary operation symbol from event topics (best-effort decode).
pub fn primary_topic_op(topics: Option<&[String]>) -> Option<String> {
    let topics = topics?;
    for raw in topics {
        if let Some(sym) = decode_topic_symbol(raw) {
            // Soroswap pair/router prefix topic.
            if sym == "SoroswapPair" || sym == "SoroswapRouter" {
                continue;
            }
            return Some(sym);
        }
    }
    None
}

/// Classify a pool or router event by its primary topic symbol.
pub fn classify_topic_op(op: &str) -> DexEventKind {
    match op {
        "swap" | "trade" | "Swap" => DexEventKind::Swap,
        "deposit" | "deposit_liquidity" | "provide_liquidity" | "join_pool" | "add" | "deposit_position" => {
            DexEventKind::AddLiquidity
        }
        "withdraw" | "withdraw_liquidity" | "exit_pool" | "remove" | "withdraw_position" => {
            DexEventKind::RemoveLiquidity
        }
        "update_reserves" | "sync" | "pool_state" | "position_update" => DexEventKind::StateUpdate,
        "add_pool" => DexEventKind::PoolDiscovery,
        "config_rewards" | "claim" | "set_protocol_fee" | "pool_gauge_switch_token" => DexEventKind::RewardsAdmin,
        _ => DexEventKind::Other,
    }
}

fn decode_topic_symbol(raw: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
    let scval = ScVal::from_xdr(&bytes, Limits::none()).ok()?;
    match scval {
        ScVal::Symbol(s) => Some(s.to_string()),
        ScVal::String(s) => Some(s.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use {super::*, stellar_xdr::curr::WriteXdr};

    fn sym(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(
            ScVal::Symbol(stellar_xdr::curr::ScSymbol::try_from(s).unwrap())
                .to_xdr(Limits::none())
                .unwrap(),
        )
    }

    #[test]
    fn classifies_aquarius_trade_as_swap() {
        assert_eq!(classify_topic_op("trade"), DexEventKind::Swap);
    }

    #[test]
    fn skips_soroswap_prefix_topic() {
        let topics = [sym("SoroswapPair"), sym("swap")];
        assert_eq!(primary_topic_op(Some(&topics)).as_deref(), Some("swap"));
    }
}

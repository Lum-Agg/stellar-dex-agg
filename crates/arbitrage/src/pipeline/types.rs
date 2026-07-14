use {crate::scanner::ArbOpportunity, router_engine::TokenId};

/// One base×bridge pair to quote (collector yield unit).
#[derive(Debug, Clone)]
pub struct BridgeScanItem {
    pub base: TokenId,
    pub bridge: TokenId,
}

#[derive(Debug, Clone)]
pub enum Event {
    BridgeScan(BridgeScanItem),
}

#[derive(Debug, Clone)]
pub enum Action {
    ExecuteOpportunity(ArbOpportunity),
}

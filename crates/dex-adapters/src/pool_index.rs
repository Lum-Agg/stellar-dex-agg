//! Map on-chain contract IDs to LumAgg `(source, pool_address)` for ledger event ingestion.

use std::collections::{HashMap, HashSet};

use market_snapshot::{ClmmPoolRefSnapshot, SourceSnapshot};

use crate::rpc::events::ContractEvent;
use crate::utils::is_contract_address;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PoolRef {
    pub source: String,
    pub pool_address: String,
}

#[derive(Debug, Clone, Default)]
pub struct KnownPoolIndex {
    by_contract: HashMap<String, PoolRef>,
}

impl KnownPoolIndex {
    pub fn rebuild(sources: &[SourceSnapshot], clmm_pool_refs: &[ClmmPoolRefSnapshot]) -> Self {
        let mut by_contract = HashMap::new();
        for source in sources {
            for pair in &source.pairs {
                if is_contract_address(&pair.pool_address) {
                    by_contract.insert(
                        pair.pool_address.clone(),
                        PoolRef {
                            source: source.source.clone(),
                            pool_address: pair.pool_address.clone(),
                        },
                    );
                }
            }
        }
        for pool in clmm_pool_refs {
            if is_contract_address(&pool.pool_address) {
                by_contract.insert(
                    pool.pool_address.clone(),
                    PoolRef {
                        source: pool.source.clone(),
                        pool_address: pool.pool_address.clone(),
                    },
                );
            }
        }
        Self { by_contract }
    }

    pub fn len(&self) -> usize {
        self.by_contract.len()
    }

    pub fn lookup_contract(&self, contract_id: &str) -> Option<&PoolRef> {
        self.by_contract.get(contract_id)
    }
}

/// Pools whose contract emitted a contract event in the indexed ledger range.
pub fn touched_pools_from_events(
    events: &[ContractEvent],
    index: &KnownPoolIndex,
) -> HashSet<PoolRef> {
    let mut touched = HashSet::new();
    for event in events {
        if event.event_type != "contract" {
            continue;
        }
        if let Some(pool) = index.lookup_contract(&event.contract_id) {
            touched.insert(pool.clone());
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use market_snapshot::TradingPairSnapshot;

    #[test]
    fn maps_events_to_known_pools() {
        let sources = vec![market_snapshot::SourceSnapshot {
            source: "soroswap".to_string(),
            pairs: vec![TradingPairSnapshot {
                token_a: "A".to_string(),
                token_b: "B".to_string(),
                pool_address: "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2"
                    .to_string(),
                fee_bps: 30,
            }],
        }];
        let index = KnownPoolIndex::rebuild(&sources, &[]);
        let events = vec![ContractEvent {
            event_type: "contract".to_string(),
            ledger: 100,
            contract_id: "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2".to_string(),
            id: "1-1".to_string(),
            tx_hash: "a".repeat(64),
            value: None,
            topic: None,
        }];
        let touched = touched_pools_from_events(&events, &index);
        assert_eq!(touched.len(), 1);
        let pool = touched.iter().next().unwrap();
        assert_eq!(pool.source, "soroswap");
    }
}

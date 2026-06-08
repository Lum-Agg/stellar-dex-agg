//! Map on-chain contract IDs to LumAgg `(source, pool_address)` for ledger
//! event ingestion.

use {
    crate::{router_events::pools_from_router_event, rpc::events::ContractEvent, utils::is_contract_address},
    market_snapshot::{ClmmPoolRefSnapshot, SourceSnapshot},
    std::collections::{HashMap, HashSet},
};

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

    fn insert_if_known(&self, touched: &mut HashSet<PoolRef>, pool_address: &str) {
        if let Some(pool) = self.lookup_contract(pool_address) {
            touched.insert(pool.clone());
        }
    }
}

/// Pools touched in the ledger range: direct pool contract events plus router
/// events where the pool id is carried in the event body (Aquarius
/// deposit/swap/ withdraw, Soroswap add/remove liquidity).
pub fn touched_pools_from_events(events: &[ContractEvent], index: &KnownPoolIndex) -> HashSet<PoolRef> {
    let mut touched = HashSet::new();
    for event in events {
        if event.event_type != "contract" {
            continue;
        }
        if let Some(pool) = index.lookup_contract(&event.contract_id) {
            touched.insert(pool.clone());
            continue;
        }
        for pool_address in pools_from_router_event(&event.contract_id, event.topic.as_deref(), event.value.as_deref())
        {
            index.insert_if_known(&mut touched, &pool_address);
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use {super::*, market_snapshot::TradingPairSnapshot};

    const POOL: &str = "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2";

    fn sample_index() -> KnownPoolIndex {
        KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "soroswap".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: POOL.to_string(),
                    fee_bps: 30,
                }],
            }],
            &[],
        )
    }

    #[test]
    fn maps_events_to_known_pools() {
        let index = sample_index();
        let events = vec![ContractEvent {
            event_type: "contract".to_string(),
            ledger: 100,
            contract_id: POOL.to_string(),
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

    #[test]
    fn maps_aquarius_router_event_to_pool() {
        use {
            crate::aquarius::AQUARIUS_ROUTER,
            base64::Engine,
            stellar_strkey::Contract,
            stellar_xdr::curr::{Limits, ScAddress, ScVal, WriteXdr},
        };

        let hash = [42u8; 32];
        let pool_id = format!("{}", Contract(hash));
        let index = KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "aquarius".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: pool_id.clone(),
                    fee_bps: 30,
                }],
            }],
            &[],
        );

        let body = ScVal::Vec(Some(stellar_xdr::curr::ScVec(
            vec![
                ScVal::Address(ScAddress::Contract(stellar_xdr::curr::ContractId(
                    stellar_xdr::curr::Hash(hash),
                ))),
                ScVal::U32(1),
            ]
            .try_into()
            .unwrap(),
        )));
        let topic = ScVal::Symbol(stellar_xdr::curr::ScSymbol::try_from("deposit").unwrap());
        let b64 = |v: &ScVal| base64::engine::general_purpose::STANDARD.encode(v.to_xdr(Limits::none()).unwrap());

        let events = vec![ContractEvent {
            event_type: "contract".to_string(),
            ledger: 100,
            contract_id: AQUARIUS_ROUTER.to_string(),
            id: "1-2".to_string(),
            tx_hash: "b".repeat(64),
            value: Some(b64(&body)),
            topic: Some(vec![b64(&topic)]),
        }];
        let touched = touched_pools_from_events(&events, &index);
        assert_eq!(touched.len(), 1);
        assert_eq!(touched.iter().next().unwrap().pool_address, pool_id);
    }
}

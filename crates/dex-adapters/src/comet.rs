//! Comet DEX adapter: Balancer-style weighted pool AMM on Soroban.
//!
//! Pools are deployed via the Comet factory (`new_c_pool` / `is_c_pool` /
//! `NEW_POOL` events). Discovery uses seed pools, optional env extras, and
//! factory `getEvents` scans; routing edges cover every token pair in each pool
//! (2–8 tokens).

use {
    crate::{
        comet_math::{self, CometRecord, STROOP_SCALAR},
        rpc::{
            events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
            SorobanRpc,
        },
        traits::*,
    },
    anyhow::{anyhow, Result},
    async_trait::async_trait,
    base64::Engine,
    std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    },
    stellar_xdr::curr as xdr,
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

/// Mainnet Comet factory (Blend deployment —
/// `blend-utils/mainnet.contracts.json`).
pub const COMET_FACTORY_MAINNET: &str = "CA2LVIPU6HJHHPPD6EDDYJTV2QEUBPGOAVJ4VIYNTMFUCRM4LFK3TJKF";

/// Seed pool(s) used when factory indexing is unavailable.
pub const COMET_SEED_POOLS: &[&str] = &["CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM"];

/// Legacy hardcoded pair (BLND/USDC); kept for tests referencing the primary
/// pool.
pub const COMET_POOLS: &[(&str, &str, &str)] = &[(
    COMET_SEED_POOLS[0],
    "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
    "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
)];

/// On-chain pool state used for local weighted-pool quotes.
#[derive(Debug, Clone)]
pub struct CometPoolQuoteState {
    pub records: HashMap<String, CometRecord>,
    pub swap_fee: i128,
}

type CometPoolState = CometPoolQuoteState;

/// Weighted-pool quote from hydrated state (Balancer V1 math).
pub fn quote_comet_pool(
    state: &CometPoolQuoteState,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
) -> Option<AdapterQuote> {
    let in_record = state.records.get(token_in)?;
    let out_record = state.records.get(token_out)?;
    let amount_out = comet_math::calc_out_given_in(in_record, out_record, amount_in as i128, state.swap_fee);
    if amount_out <= 0 {
        return None;
    }
    let price_impact_bps = (amount_in as i128 * 10_000 / (2 * in_record.balance)).min(10_000) as u32;
    Some(AdapterQuote {
        amount_out: amount_out as u128,
        fee_bps: (state.swap_fee / 1000) as u32,
        price_impact_bps,
    })
}

pub struct CometAdapter {
    rpc: Arc<SorobanRpc>,
    pairs: RwLock<Vec<AdapterTradingPair>>,
    pool_states: RwLock<HashMap<String, CometPoolState>>,
    tracked_pools: RwLock<Vec<String>>,
}

impl CometAdapter {
    pub fn new(rpc: Arc<SorobanRpc>) -> Self {
        Self {
            rpc,
            pairs: RwLock::new(Vec::new()),
            pool_states: RwLock::new(HashMap::new()),
            tracked_pools: RwLock::new(Vec::new()),
        }
    }

    fn factory_address() -> String {
        std::env::var("COMET_FACTORY").unwrap_or_else(|_| COMET_FACTORY_MAINNET.to_string())
    }

    fn extra_pool_addresses_from_env() -> Vec<String> {
        std::env::var("COMET_EXTRA_POOLS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn address_scval(addr: &str) -> Result<xdr::ScVal> {
        let hash = stellar_strkey::Contract::from_string(addr)
            .map_err(|e| anyhow!("Invalid contract address {}: {:?}", addr, e))?
            .0;
        Ok(xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(
            xdr::Hash(hash),
        ))))
    }

    /// Discover pool contract IDs: factory registry + seeds +
    /// `COMET_EXTRA_POOLS`.
    pub async fn discover_pool_addresses(&self) -> Vec<String> {
        let mut addrs: HashSet<String> = HashSet::new();
        for seed in COMET_SEED_POOLS {
            addrs.insert(seed.to_string());
        }
        for extra in Self::extra_pool_addresses_from_env() {
            addrs.insert(extra);
        }

        match self.discover_pools_from_factory_events().await {
            Ok(event_pools) => {
                info!("Comet: factory events listed {} candidate pool(s)", event_pools.len());
                addrs.extend(event_pools);
            }
            Err(e) => {
                debug!("Comet: factory event discovery failed: {}", e);
            }
        }

        let factory = Self::factory_address();
        let mut confirmed = Vec::new();
        for addr in addrs {
            if self.factory_confirms_pool(&factory, &addr).await {
                confirmed.push(addr);
            } else if COMET_SEED_POOLS.contains(&addr.as_str()) {
                // Seed pool: still track if on-chain reads succeed (factory RPC may fail).
                if self.probe_pool_contract(&addr).await {
                    confirmed.push(addr);
                }
            }
        }

        confirmed.sort();
        *self.tracked_pools.write().await = confirmed.clone();
        confirmed
    }

    async fn factory_confirms_pool(&self, factory: &str, pool: &str) -> bool {
        let args = match Self::address_scval(pool) {
            Ok(v) => vec![v],
            Err(_) => return false,
        };
        match self.rpc.simulate_call(factory, "is_c_pool", args).await {
            Ok(xdr::ScVal::Bool(true)) => true,
            Ok(_) => false,
            Err(e) => {
                debug!("Comet is_c_pool({}) failed: {}", pool, e);
                false
            }
        }
    }

    async fn probe_pool_contract(&self, pool: &str) -> bool {
        self.rpc.call_no_args(pool, "get_tokens").await.is_ok()
    }

    /// Scan recent factory contract events for `NEW_POOL` payloads.
    async fn discover_pools_from_factory_events(&self) -> Result<Vec<String>> {
        let factory = Self::factory_address();
        let latest = self.rpc.get_latest_ledger().await?.sequence;
        let window: u32 = std::env::var("COMET_FACTORY_EVENTS_LEDGER_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50_000);
        let start = latest.saturating_sub(window).max(1);

        let mut pool_hashes: HashSet<[u8; 32]> = HashSet::new();
        let mut cursor_start = start;
        while cursor_start < latest {
            let end = (cursor_start + MAX_LEDGER_SCAN_PER_REQUEST).min(latest);
            let filters = vec![EventFilterSpec {
                contract_ids: Some(vec![factory.clone()]),
                topics: None,
            }];
            let events = self
                .rpc
                .get_contract_events(cursor_start, Some(end), &filters, 10_000)
                .await?;
            for event in &events {
                let value_b64 = event
                    .value
                    .as_ref()
                    .and_then(|v| v.as_str().or_else(|| v.get("xdr").and_then(|x| x.as_str())));
                if let Some(value_b64) = value_b64 {
                    if let Some(hash) = contract_hash_from_storage_xdr(value_b64) {
                        pool_hashes.insert(hash);
                    }
                }
            }
            cursor_start = end;
        }

        Ok(pool_hashes
            .iter()
            .map(|hash| format!("{}", stellar_strkey::Contract(*hash)))
            .collect())
    }

    /// Fetch pool state for quote-time hydration.
    pub async fn fetch_pool_quote_state(&self, pool_address: &str) -> Result<CometPoolQuoteState> {
        self.fetch_pool_state(pool_address).await
    }

    async fn fetch_token_weight(&self, pool_address: &str, token_scval: xdr::ScVal) -> Result<i128> {
        let normalized = self
            .rpc
            .simulate_call(pool_address, "get_normalized_weight", vec![token_scval.clone()])
            .await;
        if let Ok(val) = normalized {
            if let Ok(w) = extract_i128(&val) {
                if w > 0 {
                    return Ok(w);
                }
            }
        }
        let denorm = self
            .rpc
            .simulate_call(pool_address, "get_denorm_weight", vec![token_scval])
            .await
            .map_err(|e| anyhow!("get_denorm_weight failed: {}", e))?;
        extract_i128(&denorm)
    }

    async fn fetch_pool_state(&self, pool_address: &str) -> Result<CometPoolState> {
        let fee_val = self
            .rpc
            .call_no_args(pool_address, "get_swap_fee")
            .await
            .map_err(|e| anyhow!("get_swap_fee failed: {}", e))?;
        let swap_fee = extract_i128(&fee_val).unwrap_or(30_000);

        let tokens_val = self
            .rpc
            .call_no_args(pool_address, "get_tokens")
            .await
            .map_err(|e| anyhow!("get_tokens failed: {}", e))?;

        let token_addrs: Vec<String> = match &tokens_val {
            xdr::ScVal::Vec(Some(vec)) => vec
                .0
                .iter()
                .filter_map(|v| crate::rpc::scval_to_address(v).ok())
                .collect(),
            _ => return Err(anyhow!("Cannot parse get_tokens result")),
        };

        if token_addrs.len() < 2 {
            return Err(anyhow!("Pool has fewer than 2 tokens"));
        }

        let mut records = HashMap::new();
        for token_addr in &token_addrs {
            let token_scval = Self::address_scval(token_addr)?;

            let balance_val = self
                .rpc
                .simulate_call(pool_address, "get_balance", vec![token_scval.clone()])
                .await
                .map_err(|e| anyhow!("get_balance failed: {}", e))?;

            let weight = self.fetch_token_weight(pool_address, token_scval).await.unwrap_or(0);
            let balance = extract_i128(&balance_val).unwrap_or(0);

            if balance > 0 && weight > 0 {
                records.insert(
                    token_addr.clone(),
                    CometRecord {
                        balance,
                        weight,
                        scalar: STROOP_SCALAR,
                    },
                );
            }
        }

        if records.len() < 2 {
            return Err(anyhow!("Pool has fewer than 2 tokens with balance"));
        }

        debug!(
            "Comet pool {}: {} tokens, fee={}",
            pool_address,
            records.len(),
            swap_fee
        );
        Ok(CometPoolState { records, swap_fee })
    }

    /// One graph edge per unordered token pair with liquidity in the pool.
    pub fn trading_pairs_from_state(pool_address: &str, state: &CometPoolState) -> Vec<AdapterTradingPair> {
        // HashMap iteration order is intentionally nondeterministic. Sort the
        // token ids so pair orientation and reserve_a/reserve_b are stable.
        let mut tokens: Vec<String> = state.records.keys().cloned().collect();
        tokens.sort();
        let fee_bps = (state.swap_fee / 1000) as u32;
        let mut pairs = Vec::new();
        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                let token_a = &tokens[i];
                let token_b = &tokens[j];
                let reserve_a = state.records.get(token_a).map(|r| r.balance as u128);
                let reserve_b = state.records.get(token_b).map(|r| r.balance as u128);
                pairs.push(AdapterTradingPair {
                    token_a: TokenId::Contract {
                        address: token_a.clone(),
                    },
                    token_b: TokenId::Contract {
                        address: token_b.clone(),
                    },
                    pool_address: pool_address.to_string(),
                    fee_bps,
                    reserve_a,
                    reserve_b,
                });
            }
        }
        pairs
    }

    async fn apply_pool_state(&self, pool_address: &str, state: CometPoolState) -> usize {
        let new_pairs = Self::trading_pairs_from_state(pool_address, &state);
        if new_pairs.is_empty() {
            return 0;
        }
        self.pool_states.write().await.insert(pool_address.to_string(), state);
        let mut pairs = self.pairs.write().await;
        pairs.retain(|p| p.pool_address != pool_address);
        let n = new_pairs.len();
        pairs.extend(new_pairs);
        n
    }

    pub async fn refresh_pool(&self, pool_address: &str) -> Result<bool> {
        let state = self.fetch_pool_state(pool_address).await?;
        Ok(self.apply_pool_state(pool_address, state).await > 0)
    }

    /// Cached weighted pool states (for Redis publish).
    pub async fn export_pool_quote_states(&self) -> Vec<(String, CometPoolQuoteState)> {
        self.pool_states
            .read()
            .await
            .iter()
            .map(|(pool, state)| (pool.clone(), state.clone()))
            .collect()
    }

    pub async fn export_pool_quote_states_for(&self, pool_addresses: &[String]) -> Vec<(String, CometPoolQuoteState)> {
        let states = self.pool_states.read().await;
        let wanted: HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
        states
            .iter()
            .filter(|(pool, _)| wanted.contains(pool.as_str()))
            .map(|(pool, state)| (pool.clone(), state.clone()))
            .collect()
    }

    pub async fn known_pool_addresses(&self) -> Vec<String> {
        let tracked = self.tracked_pools.read().await;
        if tracked.is_empty() {
            COMET_SEED_POOLS.iter().map(|s| s.to_string()).collect()
        } else {
            tracked.clone()
        }
    }
}

/// Extract a Soroban contract hash from base64-encoded storage/event XDR.
fn contract_hash_from_storage_xdr(b64: &str) -> Option<[u8; 32]> {
    let raw = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    contract_hash_from_xdr_bytes(&raw)
}

fn contract_hash_from_xdr_bytes(raw: &[u8]) -> Option<[u8; 32]> {
    const MARKER: [u8; 8] = [0, 0, 0, 0x12, 0, 0, 0, 1];
    for i in 0..=raw.len().saturating_sub(40) {
        if raw[i..].starts_with(&MARKER) {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&raw[i + 8..i + 40]);
            return Some(hash);
        }
    }
    None
}

fn extract_i128(val: &xdr::ScVal) -> Result<i128> {
    match val {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | (parts.lo as u64 as i128)),
        xdr::ScVal::U128(parts) => Ok(((parts.hi as u128) << 64 | parts.lo as u128) as i128),
        xdr::ScVal::I64(v) => Ok(*v as i128),
        xdr::ScVal::U64(v) => Ok(*v as i128),
        xdr::ScVal::U32(v) => Ok(*v as i128),
        xdr::ScVal::I32(v) => Ok(*v as i128),
        _ => Err(anyhow!("Not a number")),
    }
}

#[async_trait]
impl DexAdapter for CometAdapter {
    fn id(&self) -> &str {
        "comet"
    }

    fn name(&self) -> &str {
        "Comet"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::SorobanWeightedPool
    }

    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>> {
        let pool_addrs = self.discover_pool_addresses().await;
        let mut pairs = Vec::new();
        let mut states = HashMap::new();

        for pool_addr in &pool_addrs {
            match self.fetch_pool_state(pool_addr).await {
                Ok(state) => {
                    pairs.extend(Self::trading_pairs_from_state(pool_addr, &state));
                    states.insert(pool_addr.clone(), state);
                }
                Err(e) => {
                    warn!("Comet pool {} fetch failed: {}", pool_addr, e);
                }
            }
        }

        info!("Comet: {} pools, {} pair edges loaded", states.len(), pairs.len());
        *self.pairs.write().await = pairs.clone();
        *self.pool_states.write().await = states;
        Ok(pairs)
    }

    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>> {
        let states = self.pool_states.read().await;
        let state = match states.get(pool_address) {
            Some(s) => s,
            None => return Ok(None),
        };

        Ok(quote_comet_pool(
            state,
            &token_in.canonical(),
            &token_out.canonical(),
            amount_in,
        ))
    }

    async fn build_swap_op(
        &self,
        _token_in: &TokenId,
        _token_out: &TokenId,
        _amount_in: u128,
        _min_amount_out: u128,
        pool_address: &str,
    ) -> Result<SwapOperation> {
        Ok(SwapOperation::SorobanInvoke {
            contract_id: pool_address.to_string(),
            function_name: "swap_exact_amount_in".to_string(),
            args_xdr: vec![],
        })
    }

    async fn health_check(&self) -> bool {
        if let Some(seed) = COMET_SEED_POOLS.first() {
            self.fetch_pool_state(seed).await.is_ok()
        } else {
            false
        }
    }

    async fn refresh_reserves(&self) -> Result<usize> {
        let pools = self.known_pool_addresses().await;
        let mut updated = 0usize;
        for pool_addr in &pools {
            if self.refresh_pool(pool_addr).await? {
                updated += 1;
            }
        }
        Ok(updated)
    }

    async fn get_cached_pairs(&self) -> Vec<AdapterTradingPair> {
        self.pairs.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trading_pairs_from_state_emits_all_token_pairs() {
        let pool = COMET_SEED_POOLS[0];
        let mut records = HashMap::new();
        records.insert(
            "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV".to_string(),
            CometRecord {
                balance: 1_000_000,
                weight: 1,
                scalar: STROOP_SCALAR,
            },
        );
        records.insert(
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".to_string(),
            CometRecord {
                balance: 2_000_000,
                weight: 4,
                scalar: STROOP_SCALAR,
            },
        );
        let state = CometPoolState {
            records,
            swap_fee: 30_000,
        };
        let pairs = CometAdapter::trading_pairs_from_state(pool, &state);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].pool_address, pool);
        assert_eq!(pairs[0].token_a.canonical(), "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75");
        assert_eq!(pairs[0].token_b.canonical(), "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV");
        assert_eq!(pairs[0].reserve_a, Some(2_000_000));
        assert_eq!(pairs[0].reserve_b, Some(1_000_000));
        assert_eq!(pairs[0].fee_bps, 30);
    }

    #[test]
    fn three_token_pool_has_three_edges() {
        let mut records = HashMap::new();
        for (addr, bal) in [("A", 100), ("B", 200), ("C", 300)] {
            records.insert(
                addr.to_string(),
                CometRecord {
                    balance: bal,
                    weight: 1,
                    scalar: STROOP_SCALAR,
                },
            );
        }
        let state = CometPoolState {
            records,
            swap_fee: 30_000,
        };
        let pairs = CometAdapter::trading_pairs_from_state("POOL", &state);
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn contract_hash_from_storage_xdr_finds_embedded_contract() {
        let hash = [7u8; 32];
        let mut raw = vec![0u8; 48];
        raw[8..8 + 8].copy_from_slice(&[0, 0, 0, 0x12, 0, 0, 0, 1]);
        raw[16..48].copy_from_slice(&hash);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        assert_eq!(contract_hash_from_storage_xdr(&b64), Some(hash));
    }
}

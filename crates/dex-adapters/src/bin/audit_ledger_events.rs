//! Audit getEvents → touched pools on live chain (pool + router parsing).
//!
//!   RPC_URL=http://127.0.0.1:8003 REDIS_URL=redis://:pass@127.0.0.1:6379/ \
//!     cargo run -p dex-adapters --release --bin audit-ledger-events

use {
    dex_adapters::{
        aquarius::AQUARIUS_ROUTER,
        pool_index::{touched_pools_from_events, KnownPoolIndex},
        router_events::pools_from_router_event,
        rpc::{events::ContractEvent, SorobanRpc},
        soroswap::SOROSWAP_ROUTER,
    },
    market_snapshot::MarketSnapshot,
    std::collections::{HashMap, HashSet},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://:REDISzlg153@127.0.0.1:6379/".into());
    let ledgers: u32 = std::env::var("AUDIT_LEDGERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let snap = load_snapshot_from_redis(&redis_url).await?;
    let index = KnownPoolIndex::rebuild(&snap.sources, &snap.clmm_pool_refs);
    println!(
        "snapshot sources={} clmm_refs={} known_pools={}",
        snap.sources.len(),
        snap.clmm_pool_refs.len(),
        index.len()
    );

    let rpc = SorobanRpc::new(&rpc_url, "Public Global Stellar Network ; September 2015");
    let latest = rpc.get_latest_ledger().await?.sequence;
    println!("latest_ledger={latest} scanning_last={ledgers}\n");

    let mut totals = AuditTotals::default();
    let mut unique_pools_by_source: HashMap<String, HashSet<String>> = HashMap::new();
    for ledger in latest.saturating_sub(ledgers - 1)..=latest {
        let events = fetch_ledger_events(&rpc, ledger).await?;
        let ledger_stats = audit_ledger(&events, &index);
        for pool in touched_pools_from_events(&events, &index) {
            unique_pools_by_source
                .entry(pool.source.clone())
                .or_default()
                .insert(pool.pool_address);
        }
        totals.merge(&ledger_stats);
        if ledger_stats.has_activity() {
            print_ledger(ledger, &ledger_stats);
        }
    }

    println!("=== SUMMARY (last {ledgers} ledgers) ===");
    totals.print();
    print_unique_pools_by_dex(&unique_pools_by_source);
    Ok(())
}

#[derive(Default)]
struct AuditTotals {
    ledgers_with_activity: u32,
    contract_events: usize,
    pool_contract_events: usize,
    router_events: usize,
    router_parsed: usize,
    router_parse_failed: usize,
    touched_pool_only: usize,
    touched_full: usize,
    touched_via_router_only: usize,
    by_source: HashMap<String, usize>,
}

impl AuditTotals {
    fn merge(&mut self, s: &LedgerStats) {
        if !s.has_activity() {
            return;
        }
        self.ledgers_with_activity += 1;
        self.contract_events += s.contract_events;
        self.pool_contract_events += s.pool_contract_events;
        self.router_events += s.router_events;
        self.router_parsed += s.router_parsed;
        self.router_parse_failed += s.router_parse_failed;
        self.touched_pool_only += s.touched_pool_only.len();
        self.touched_full += s.touched_full.len();
        self.touched_via_router_only += s.touched_via_router_only.len();
        for (src, n) in &s.by_source {
            *self.by_source.entry(src.clone()).or_default() += n;
        }
    }

    fn print(&self) {
        println!("ledgers_with_dex_activity: {}", self.ledgers_with_activity);
        println!("contract_events: {}", self.contract_events);
        println!("  pool_contract_events (contractId in index): {}", self.pool_contract_events);
        println!("  router_events (aquarius/soroswap): {}", self.router_events);
        println!("  router_parsed_to_pool: {}", self.router_parsed);
        println!("  router_parse_failed: {}", self.router_parse_failed);
        println!("unique_touched pool_only: {}", self.touched_pool_only);
        println!("unique_touched full (pool+router): {}", self.touched_full);
        println!("extra from router path only: {}", self.touched_via_router_only);
        if self.touched_via_router_only > 0 {
            println!("  -> router parsing recovered pools pool-only would miss");
        } else {
            println!("  -> router path added no new pools (pool events cover same txs)");
        }
        println!("by_source: {:?}", self.by_source);
    }
}

struct LedgerStats {
    contract_events: usize,
    pool_contract_events: usize,
    router_events: usize,
    router_parsed: usize,
    router_parse_failed: usize,
    touched_pool_only: HashSet<String>,
    touched_full: HashSet<String>,
    touched_via_router_only: HashSet<String>,
    by_source: HashMap<String, usize>,
}

impl LedgerStats {
    fn has_activity(&self) -> bool {
        !self.touched_full.is_empty() || self.router_events > 0 || self.pool_contract_events > 0
    }
}

fn audit_ledger(events: &[ContractEvent], index: &KnownPoolIndex) -> LedgerStats {
    let mut stats = LedgerStats {
        contract_events: 0,
        pool_contract_events: 0,
        router_events: 0,
        router_parsed: 0,
        router_parse_failed: 0,
        touched_pool_only: HashSet::new(),
        touched_full: HashSet::new(),
        touched_via_router_only: HashSet::new(),
        by_source: HashMap::new(),
    };

    for event in events {
        if event.event_type != "contract" {
            continue;
        }
        stats.contract_events += 1;

        if index.lookup_contract(&event.contract_id).is_some() {
            stats.pool_contract_events += 1;
            stats
                .touched_pool_only
                .insert(event.contract_id.clone());
        }

        let is_router = event.contract_id == AQUARIUS_ROUTER || event.contract_id == SOROSWAP_ROUTER;
        if is_router {
            stats.router_events += 1;
            let parsed = pools_from_router_event(
                &event.contract_id,
                event.topic.as_deref(),
                event.value.as_deref(),
            );
            if parsed.is_empty() {
                stats.router_parse_failed += 1;
            } else {
                stats.router_parsed += parsed.len();
            }
        }
    }

    let full = touched_pools_from_events(events, index);
    for pool in &full {
        stats.touched_full.insert(pool.pool_address.clone());
        *stats.by_source.entry(pool.source.clone()).or_default() += 1;
    }

    for addr in &stats.touched_full {
        if !stats.touched_pool_only.contains(addr) {
            stats.touched_via_router_only.insert(addr.clone());
        }
    }

    stats
}

fn print_ledger(ledger: u32, s: &LedgerStats) {
    println!(
        "ledger {ledger}: events={} pool_hits={} router_ev={} router_parsed={} router_fail={} \
         touched pool_only={} full={} router_only_extra={}",
        s.contract_events,
        s.pool_contract_events,
        s.router_events,
        s.router_parsed,
        s.router_parse_failed,
        s.touched_pool_only.len(),
        s.touched_full.len(),
        s.touched_via_router_only.len(),
    );
    if !s.touched_via_router_only.is_empty() {
        for addr in &s.touched_via_router_only {
            println!("  +router {addr}");
        }
    }
}

async fn fetch_ledger_events(rpc: &SorobanRpc, ledger: u32) -> anyhow::Result<Vec<ContractEvent>> {
    use dex_adapters::rpc::events::{EventFilterSpec, DEFAULT_EVENTS_PAGE_LIMIT};

    let filters = vec![EventFilterSpec {
        contract_ids: None,
        topics: Some(vec![vec!["**".to_string()]]),
    }];
    rpc.get_contract_events(ledger, Some(ledger + 1), &filters, DEFAULT_EVENTS_PAGE_LIMIT)
        .await
}

fn print_unique_pools_by_dex(by_source: &HashMap<String, HashSet<String>>) {
    if by_source.is_empty() {
        println!("unique_pools_by_dex: (none)");
        return;
    }
    let mut sources: Vec<_> = by_source.keys().cloned().collect();
    sources.sort();
    let total_unique: usize = by_source.values().map(|s| s.len()).sum();
    println!(
        "unique_pools_by_dex: {} dex sources, {} unique pools total",
        sources.len(),
        total_unique
    );
    for src in &sources {
        println!("  {src}: {} unique pools", by_source[src].len());
    }
}

async fn load_snapshot_from_redis(redis_url: &str) -> anyhow::Result<MarketSnapshot> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    let current: String = redis::cmd("GET")
        .arg("lumagg:snapshot:current")
        .query_async(&mut conn)
        .await?;
    let raw: String = redis::cmd("GET")
        .arg(format!("lumagg:snapshot:data:{current}"))
        .query_async(&mut conn)
        .await?;
    Ok(serde_json::from_str(&raw)?)
}
